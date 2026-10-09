#!/usr/bin/env python3
"""第三方许可清单生成器(离线,issue #56)。

解析 lockfile 并从本机缓存提取许可标识,生成 docs/third-party-licenses.md:

- Rust workspace 依赖:仓库根 Cargo.lock(约 393 包)
- 桌面应用独立 Rust 依赖:apps/desktop/src-tauri/Cargo.lock(src-tauri 被
  workspace exclude,发布桌面包实际携带)
- npm 锁定依赖:apps/desktop/package-lock.json

许可来源全部离线:
- Rust:优先 $CARGO_HOME/registry/src/<registry>/<name>-<version>/Cargo.toml,
  其次 registry/cache 里的 .crate(tar.gz,读内嵌 Cargo.toml);workspace 成员
  crate 读仓库内 Cargo.toml。
- npm:node_modules/<path>/package.json 的 license 字段。

任何拿不到许可的包标记 unknown 并在文档中单列;脚本不联网拉取许可全文。
输出为纯函数:输入不变则逐字节一致(--check 可校验)。

用法:
  python3 scripts/generate-license-inventory.py                # 生成文档
  python3 scripts/generate-license-inventory.py --check        # 校验现有文档是否与重跑一致
  python3 scripts/generate-license-inventory.py --self-check   # 内置最小自检
"""

from __future__ import annotations

import argparse
import base64
import gzip
import hashlib
import io
import json
import os
import re
import sys
import tarfile
from dataclasses import dataclass, field
from pathlib import Path

try:
    import tomllib  # Python >= 3.11
except ImportError:  # pragma: no cover
    tomllib = None

# 宽松许可白名单(SPFIX 标识,比较时大写)。按 issue #56 要求:
# MIT / Apache-2.0 / BSD 家族 / ISC / MIT 变体;其余一律进"待审查"名单。
PERMISSIVE_IDS = {
    "MIT",
    "MIT-0",
    "MIT-NO-FALSE-LIBS-ONLY",  # MIT 变体(rare)
    "APACHE-2.0",
    "BSD-1-CLAUSE",
    "BSD-2-CLAUSE",
    "BSD-2-CLAUSE-FREEBSD",
    "BSD-2-CLAUSE-NETBSD",
    "BSD-2-CLAUSE-PATENT",
    "BSD-3-CLAUSE",
    "BSD-3-CLAUSE-CLEAR",
    "BSD-3-CLAUSE-LBNL",
    "BSD-3-CLAUSE-OPEN-MPI",
    "BSD-4-CLAUSE-SHORTENED",
    "ISC",
    "0BSD",
}

# 本仓库自有 crate(非第三方)。
PROJECT_CRATE_NAMES = {"wardrobe-core", "wardrobe-cli", "wardrobe-desktop"}

UNKNOWN = "unknown"

SCOPE_WORKSPACE = "Rust workspace"
SCOPE_DESKTOP = "桌面 src-tauri"
SCOPE_NPM = "npm desktop"

STATUS_MEMBER_TOML = "仓库成员 Cargo.toml"
STATUS_MEMBER_NO_LICENSE = "本项目 crate,Cargo.toml 未声明 license(非第三方)"
STATUS_REGISTRY_SRC = "registry 源码缓存 Cargo.toml"
STATUS_REGISTRY_CRATE = "registry .crate 缓存"
STATUS_NPM_PKG_JSON = "node_modules package.json"
STATUS_NPM_CACHE_TARBALL = "npm 缓存 tarball package.json"
STATUS_NPM_LICENSE_FILE = "node_modules 仅有 LICENSE 文件(标识待查)"
STATUS_MISSING_CACHE = "本地缓存缺失(unknown)"
STATUS_NO_LICENSE_FIELD = "元数据无 license 字段(unknown)"


@dataclass
class Entry:
    name: str
    version: str
    license: str
    status: str
    note: str = ""

    @property
    def is_unknown(self) -> bool:
        return self.license == UNKNOWN

    @property
    def is_project_crate(self) -> bool:
        return self.name in PROJECT_CRATE_NAMES and "本项目 crate" in self.status

    @property
    def needs_review(self) -> bool:
        if self.is_project_crate:
            return False  # 自有 crate 不属于第三方审查范围
        if self.is_unknown:
            return True
        return not is_permissive_expression(self.license)


# ---------------------------------------------------------------------------
# SPDX 表达式分类
# ---------------------------------------------------------------------------

def _normalize_token(token: str) -> str:
    token = token.strip().strip("()").strip()
    # 去掉 WITH 例外子句(Apache-2.0 WITH LLVM-exception)
    token = re.split(r"\s+WITH\s+", token, maxsplit=1, flags=re.IGNORECASE)[0]
    return token.strip().strip("()").strip().upper()


def is_permissive_expression(expression: str) -> bool:
    """全部 AND 分支都存在一个宽松 OR 选项时才视为宽松。

    "(MIT OR Apache-2.0) AND Unicode-DFS-2016" -> False(Unicode 分支无宽松选项,
    保守列入待审查,供人工确认;不代表必然不兼容)。
    """
    if not expression or expression == UNKNOWN:
        return False
    and_branches = re.split(r"\s+AND\s+", expression, flags=re.IGNORECASE)
    for branch in and_branches:
        or_options = re.split(r"\s+OR\s+|/", branch.strip(), flags=re.IGNORECASE)
        if not any(_normalize_token(opt) in PERMISSIVE_IDS for opt in or_options if opt.strip()):
            return False
    return True


# ---------------------------------------------------------------------------
# Cargo.lock / crate 元数据
# ---------------------------------------------------------------------------

def parse_cargo_lock(path: Path) -> list[dict]:
    """返回 [{name, version, source}]。tomllib 不可用时退回轻量解析。"""
    text = path.read_text(encoding="utf-8")
    if tomllib is not None:
        data = tomllib.loads(text)
        return [
            {
                "name": p["name"],
                "version": p["version"],
                "source": p.get("source"),
            }
            for p in data.get("package", [])
        ]
    packages = []
    for block in re.split(r"\n\[\[package\]\]", text)[1:]:
        name_m = re.search(r'^name\s*=\s*"([^"]+)"', block, re.MULTILINE)
        ver_m = re.search(r'^version\s*=\s*"([^"]+)"', block, re.MULTILINE)
        src_m = re.search(r'^source\s*=\s*"([^"]+)"', block, re.MULTILINE)
        if name_m and ver_m:
            packages.append(
                {
                    "name": name_m.group(1),
                    "version": ver_m.group(1),
                    "source": src_m.group(1) if src_m else None,
                }
            )
    return packages


def license_from_cargo_toml_text(text: str) -> tuple[str, str]:
    """从 Cargo.toml 文本提取 (license, license-file)。"""
    if tomllib is not None:
        try:
            data = tomllib.loads(text)
            pkg = data.get("package", {})
            return pkg.get("license", "") or "", pkg.get("license-file", "") or ""
        except Exception:
            pass
    lic_m = re.search(r'^license\s*=\s*"([^"]*)"', text, re.MULTILINE)
    file_m = re.search(r'^license-file\s*=\s*"([^"]*)"', text, re.MULTILINE)
    return (lic_m.group(1) if lic_m else "", file_m.group(1) if file_m else "")


def find_registry_dirs(cargo_homes: list[Path]) -> tuple[list[Path], list[Path]]:
    """收集各 cargo home 下 registry 的 src/ 与 cache/ 目录。"""
    src_dirs: list[Path] = []
    cache_dirs: list[Path] = []
    for home in cargo_homes:
        reg = home / "registry"
        if not reg.is_dir():
            continue
        for entry in sorted(reg.glob("src/*/")):
            if entry.is_dir() and entry not in src_dirs:
                src_dirs.append(entry)
        for entry in sorted(reg.glob("cache/*/")):
            if entry.is_dir() and entry not in cache_dirs:
                cache_dirs.append(entry)
    return src_dirs, cache_dirs


def _crate_dir_name(name: str, version: str) -> str:
    return f"{name}-{version}"


def resolve_crate_license(
    name: str,
    version: str,
    src_dirs: list[Path],
    cache_dirs: list[Path],
) -> tuple[str, str]:
    """离线解析一个 crates.io 包的许可标识,返回 (license, status)。"""
    dir_name = _crate_dir_name(name, version)
    for src_root in src_dirs:
        toml_path = src_root / dir_name / "Cargo.toml"
        if toml_path.is_file():
            try:
                text = toml_path.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            license_id, license_file = license_from_cargo_toml_text(text)
            if license_id:
                return license_id, STATUS_REGISTRY_SRC
            if license_file:
                return UNKNOWN, f"registry 源码缓存仅有 license-file({license_file})"
            return UNKNOWN, STATUS_NO_LICENSE_FIELD
    for cache_root in cache_dirs:
        crate_path = cache_root / f"{dir_name}.crate"
        if crate_path.is_file():
            try:
                with tarfile.open(crate_path, "r:gz") as tar:
                    for member in tar.getmembers():
                        parts = member.name.split("/")
                        if len(parts) == 2 and parts[1] == "Cargo.toml":
                            fh = tar.extractfile(member)
                            if fh is None:
                                continue
                            license_id, license_file = license_from_cargo_toml_text(
                                fh.read().decode("utf-8", errors="replace")
                            )
                            if license_id:
                                return license_id, STATUS_REGISTRY_CRATE
                            if license_file:
                                return UNKNOWN, f".crate 内仅有 license-file({license_file})"
                            return UNKNOWN, STATUS_NO_LICENSE_FIELD
            except (tarfile.TarError, OSError, EOFError):
                continue
    return UNKNOWN, STATUS_MISSING_CACHE


def resolve_member_license(repo_root: Path, name: str, version: str, manifest_candidates: list[Path]) -> tuple[str, str]:
    for manifest in manifest_candidates:
        if manifest.is_file():
            license_id, license_file = license_from_cargo_toml_text(
                manifest.read_text(encoding="utf-8", errors="replace")
            )
            if license_id:
                return license_id, STATUS_MEMBER_TOML
            if license_file:
                return UNKNOWN, f"本项目 crate 仅有 license-file({license_file})"
            return UNKNOWN, STATUS_MEMBER_NO_LICENSE
    return UNKNOWN, STATUS_MISSING_CACHE


def build_rust_inventory(
    lock_path: Path,
    repo_root: Path,
    cargo_homes: list[Path],
) -> list[Entry]:
    packages = parse_cargo_lock(lock_path)
    seen: set[tuple[str, str]] = set()
    entries: list[Entry] = []
    src_dirs, cache_dirs = find_registry_dirs(cargo_homes)
    for pkg in packages:
        key = (pkg["name"], pkg["version"])
        if key in seen:
            continue
        seen.add(key)
        name, version = key
        if pkg["source"] is None:
            # Cargo.lock 中无 source 的条目是 workspace 成员或路径依赖。
            candidates = [
                repo_root / "crates" / name / "Cargo.toml",
                repo_root / "apps" / "desktop" / "src-tauri" / "Cargo.toml",
            ]
            license_id, status = resolve_member_license(repo_root, name, version, candidates)
        elif pkg["source"] and pkg["source"].startswith("registry+"):
            license_id, status = resolve_crate_license(name, version, src_dirs, cache_dirs)
        else:
            license_id, status = UNKNOWN, f"非 registry 来源({pkg['source'] or '本地路径依赖'})"
        entries.append(Entry(name=name, version=version, license=license_id, status=status))
    entries.sort(key=lambda e: (e.name.lower(), e.version))
    return entries


# ---------------------------------------------------------------------------
# npm package-lock.json
# ---------------------------------------------------------------------------

def parse_npm_lock(path: Path) -> list[dict]:
    """lockfileVersion 2/3 的 packages 映射 -> [{name, version, relkey, resolved}]。"""
    data = json.loads(path.read_text(encoding="utf-8"))
    packages = []
    pkgs = data.get("packages", {})
    if not pkgs and "dependencies" in data:  # lockfileVersion 1
        for name, info in data["dependencies"].items():
            packages.append(
                {
                    "name": name,
                    "version": info.get("version", ""),
                    "relkey": f"node_modules/{name}",
                    "resolved": info.get("resolved", ""),
                }
            )
        return packages
    for key, info in pkgs.items():
        if not key:
            continue  # 根包(本项目)
        name = key.rsplit("node_modules/", 1)[-1]
        if not name:
            continue
        packages.append(
            {
                "name": name,
                "version": info.get("version", ""),
                "relkey": key,
                "resolved": info.get("resolved", ""),
            }
        )
    return packages


def _license_value_to_str(value: object) -> str:
    if isinstance(value, str):
        return value.strip()
    if isinstance(value, dict):
        t = value.get("type")
        return t.strip() if isinstance(t, str) else ""
    if isinstance(value, list):
        parts = [v.get("type", "") for v in value if isinstance(v, dict)]
        parts = [p.strip() for p in parts if isinstance(p, str) and p.strip()]
        return " OR ".join(parts) if parts else ""
    return ""


def normalize_npm_license(raw: str) -> str:
    raw = raw.strip()
    if len(raw) >= 2 and raw[0] == "(" and raw[-1] == ")":
        raw = raw[1:-1].strip()
    return raw


def resolve_npm_license(npm_root: Path, relkey: str) -> tuple[str, str]:
    # relkey 形如 "node_modules/@scope/pkg" 或 "node_modules/a/node_modules/b";
    # npm_root 指向 node_modules 本身,去掉首个 "node_modules/" 前缀再拼接。
    rel = relkey.split("node_modules/", 1)[1]
    pkg_json = npm_root / rel / "package.json"
    if pkg_json.is_file():
        try:
            data = json.loads(pkg_json.read_text(encoding="utf-8", errors="replace"))
        except json.JSONDecodeError:
            return UNKNOWN, "node_modules package.json 解析失败(unknown)"
        raw = _license_value_to_str(data.get("license"))
        if raw:
            normalized = normalize_npm_license(raw)
            if normalized.upper() == "UNLICENSED":
                return UNKNOWN, "声明 UNLICENSED(unknown)"
            return normalized, STATUS_NPM_PKG_JSON
        lic_dir = npm_root / rel
        for cand in ("LICENSE", "LICENSE.md", "LICENSE.txt", "LICENCE", "COPYING"):
            if (lic_dir / cand).is_file():
                return UNKNOWN, STATUS_NPM_LICENSE_FILE
        return UNKNOWN, STATUS_NO_LICENSE_FIELD
    return UNKNOWN, "node_modules 目录缺失(unknown)"


def build_npm_cache_index(cache_roots: list[Path]) -> dict[str, Path]:
    """扫描 npm cacache index-v5,返回 resolved tarball URL -> 内容块路径。

    npm ci 只写 tarball 缓存(键形如 make-fetch-happen:request-cache:<url>),
    tarball 内嵌 package.json 含 license 字段,可作为 node_modules 缺失包
    (如其他平台 optional 依赖)的离线许可来源。
    """
    url_to_content: dict[str, Path] = {}
    for root in cache_roots:
        index_dir = root / "_cacache" / "index-v5"
        if not index_dir.is_dir():
            continue
        for index_file in sorted(index_dir.rglob("*")):
            if not index_file.is_file():
                continue
            try:
                lines = index_file.read_text(encoding="utf-8", errors="replace").splitlines()
            except OSError:
                continue
            for line in lines:
                if "\t" not in line:
                    continue
                payload = line.split("\t", 1)[1]
                try:
                    entry = json.loads(payload)
                except json.JSONDecodeError:
                    continue
                key = entry.get("key", "")
                integrity = entry.get("integrity", "")
                if not key.endswith(".tgz") or not integrity.startswith(("sha512-", "sha1-")):
                    continue
                url = key.split("request-cache:", 1)[-1]
                algo, b64 = integrity.split("-", 1)
                try:
                    hex_digest = base64.b64decode(b64).hex()
                except Exception:
                    continue
                content = (
                    root / "_cacache" / "content-v2" / algo
                    / hex_digest[:2] / hex_digest[2:4] / hex_digest[4:]
                )
                if content.is_file():
                    url_to_content[url] = content
    return url_to_content


def license_from_npm_tarball(content_path: Path) -> tuple[str, str]:
    """从缓存 tarball 提取 (license, package.json 里的版本)。"""
    try:
        with tarfile.open(content_path, "r:gz") as tar:
            best = None
            for member in tar.getmembers():
                if member.name.split("/")[-1] != "package.json":
                    continue
                depth = member.name.count("/")
                if best is None or depth < best[0]:
                    best = (depth, member)
            if best is None:
                return UNKNOWN, ""
            fh = tar.extractfile(best[1])
            if fh is None:
                return UNKNOWN, ""
            data = json.loads(fh.read().decode("utf-8", errors="replace"))
        raw = _license_value_to_str(data.get("license"))
        version = str(data.get("version", ""))
        if raw:
            return normalize_npm_license(raw), version
        return UNKNOWN, version
    except (tarfile.TarError, OSError, json.JSONDecodeError):
        return UNKNOWN, ""


def build_npm_inventory(
    lock_path: Path,
    npm_root: Path,
    cache_roots: list[Path] | None = None,
) -> list[Entry]:
    cache_index = build_npm_cache_index(cache_roots or [])
    entries = []
    seen: set[tuple[str, str]] = set()
    for pkg in parse_npm_lock(lock_path):
        key = (pkg["name"], pkg["version"])
        if key in seen:
            continue
        seen.add(key)
        license_id, status = resolve_npm_license(npm_root, pkg["relkey"])
        if license_id == UNKNOWN and pkg["resolved"]:
            content = cache_index.get(pkg["resolved"])
            if content is not None:
                tarball_license, tarball_version = license_from_npm_tarball(content)
                if tarball_license != UNKNOWN and tarball_version and tarball_version != pkg["version"]:
                    pass  # 版本不一致时不采信缓存内容
                elif tarball_license != UNKNOWN:
                    license_id = tarball_license
                    status = STATUS_NPM_CACHE_TARBALL
        entries.append(
            Entry(name=pkg["name"], version=pkg["version"], license=license_id, status=status)
        )
    entries.sort(key=lambda e: (e.name.lower(), e.version))
    return entries


# ---------------------------------------------------------------------------
# Markdown 渲染(纯函数,重跑逐字节一致)
# ---------------------------------------------------------------------------

def _esc(text: str) -> str:
    return text.replace("|", "\\|").replace("\n", " ")


def _table(headers: list[str], rows: list[list[str]]) -> str:
    out = ["| " + " | ".join(headers) + " |", "|" + "|".join([" --- "] * len(headers)) + "|"]
    for row in rows:
        out.append("| " + " | ".join(_esc(c) for c in row) + " |")
    return "\n".join(out)


def _sha256_short(path: Path) -> str:
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    return digest[:16]


def _entry_rows(entries: list[Entry]) -> list[list[str]]:
    return [[e.name, e.version, e.license, e.status] for e in entries]


def render_document(
    repo_root: Path,
    sections: list[dict],
) -> str:
    """sections: [{scope, entries, lock_path, lock_label}]"""
    lines: list[str] = []
    lines.append("# 第三方许可清单")
    lines.append("")
    lines.append(
        "本清单由 `scripts/generate-license-inventory.py` 从 lockfile 离线生成,"
        "覆盖 Rust workspace、桌面应用(src-tauri,被 workspace exclude、发布桌面包实际携带)"
        "与 npm 桌面前端三部分。许可标识取自本机 cargo registry 缓存与 node_modules 元数据,"
        "不联网拉取许可全文;缓存缺失的包标记 `unknown` 并在下方单列。"
    )
    lines.append("")
    lines.append("复跑方式:`python3 scripts/generate-license-inventory.py`(输出对相同输入逐字节一致,"
                 "`--check` 可校验)。")
    lines.append("")
    lines.append("## 输入基线")
    lines.append("")
    lines.append(_table(
        ["范围", "lockfile", "SHA256(前 16 位)", "包数"],
        [
            [s["scope"], _esc(s["lock_label"]), _sha256_short(s["lock_path"]), str(len(s["entries"]))]
            for s in sections
        ],
    ))
    lines.append("")
    lines.append("> 本清单是元数据盘点,不是法律意见;许可全文以各上游仓库发布文本为准。")
    lines.append("")

    total = sum(len(s["entries"]) for s in sections)
    total_unknown = sum(1 for s in sections for e in s["entries"] if e.is_unknown)
    total_review = sum(1 for s in sections for e in s["entries"] if e.needs_review)

    lines.append("## 汇总")
    lines.append("")
    lines.append(_table(
        ["范围", "包数", "许可已知", "unknown", "待审查(非 MIT/Apache/BSD/ISC 家族)"],
        [
            [
                s["scope"],
                str(len(s["entries"])),
                str(sum(1 for e in s["entries"] if not e.is_unknown)),
                str(sum(1 for e in s["entries"] if e.is_unknown)),
                str(sum(1 for e in s["entries"] if e.needs_review)),
            ]
            for s in sections
        ]
        + [["合计", str(total), str(total - total_unknown), str(total_unknown), str(total_review)]],
    ))
    lines.append("")

    for s in sections:
        entries: list[Entry] = s["entries"]
        lines.append(f"## {s['scope']}依赖许可({len(entries)} 包)")
        lines.append("")
        lines.append(_table(["包名", "版本", "许可标识", "来源状态"], _entry_rows(entries)))
        lines.append("")

    # 待审查清单
    lines.append("## 非宽松许可依赖(许可证兼容审查名单)")
    lines.append("")
    lines.append(
        "以下依赖的许可标识不在 MIT / Apache-2.0 / BSD 家族 / ISC / MIT 变体白名单内"
        "(含 unknown),按 issue #56 要求单独列出供许可证选择与发行审查。"
        "注意:此分类刻意保守——`Unicode-3.0`、`CC0-1.0`、`MPL-2.0`、`zlib` 等"
        "事实宽松或弱 copyleft 许可也会列入,列入不代表必然不兼容,发行前需逐项确认。"
        "本仓库自有 crate 不属于第三方,不列入本节(见 unknown 待补全)。"
    )
    lines.append("")
    review_rows = []
    for s in sections:
        for e in s["entries"]:
            if e.needs_review:
                review_rows.append([s["scope"], e.name, e.version, e.license, e.status])
    if review_rows:
        lines.append(_table(["范围", "包名", "版本", "许可标识", "来源状态"], review_rows))
    else:
        lines.append("(无)")
    lines.append("")

    # unknown 清单
    lines.append("## unknown 待补全")
    lines.append("")
    unknown_rows = []
    for s in sections:
        for e in s["entries"]:
            if e.is_unknown:
                unknown_rows.append([s["scope"], e.name, e.version, e.status])
    if unknown_rows:
        lines.append(_table(["范围", "包名", "版本", "原因"], unknown_rows))
        lines.append("")
        lines.append(
            "补全方法:在联网环境执行 `cargo fetch`(Rust)与 `npm ci`(npm)后复跑本脚本;"
            "或运行 `cargo about generate` / `license-checker` 等工具交叉核对。"
            "补全前不得视为完整发行清单。"
        )
        lines.append("")
        lines.append(
            "注:标注「本项目 crate」的条目是本仓库自有代码而非第三方依赖,"
            "其 license 字段未声明属于产品许可证选择事项(见 2026-10-07 MVP 发布门槛"
            "`docs/verification/mvp-release-20261007.md`),不随第三方补全解决。"
        )
    else:
        lines.append("(无)")
    lines.append("")

    lines.append(_render_msvc_section())
    return "\n".join(lines) + "\n"


def _render_msvc_section() -> str:
    return "\n".join([
        "## Windows MSVC CRT 再分发条款",
        "",
        "Windows 构建使用 `x86_64-pc-windows-msvc` 目标(见 2026-10-07 MVP 发布门槛记录,"
        "`docs/verification/mvp-release-20261007.md`),产物依赖 Microsoft Visual C++ 运行库"
        "(MSVC CRT:`vcruntime140.dll`、`vcruntime140_1.dll`、`msvcp140.dll` 等)与"
        "通用 C 运行库(UCRT:`ucrtbase.dll` 及 `api-ms-win-*` 转发 DLL)。",
        "",
        "- MSVC CRT 是**微软许可软件,不是开源软件**,不在上方许可清单内;"
        "随安装包再分发必须遵循微软的软件许可条款。",
        "- 微软的 Visual Studio / Visual C++ Redistributable 许可条款允许在遵守条款的前提下"
        "随应用再分发 CRT(例如捆绑 `vc_redist.x64.exe`,或按条款直接分发所需 DLL):"
        "不得修改、不得绕过许可与激活机制、须随包附带相应许可与声明信息,并仅以运行应用所需为限。",
        "- 若 Tauri/NSIS 打包改为静态链接 CRT(`/MT`)或要求用户自行安装 VC++ 可再发行组件包,"
        "再分发义务随之变化,打包方式变更时必须复核本节。",
        "- 官方文档(细节以微软现行条款为准):",
        "  - 再分发 Visual C++ 文件:<https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files>",
        "  - 确定可再分发组件:<https://learn.microsoft.com/cpp/windows/determining-which-dlls-to-redistribute>",
        "  - 受支持 VC++ 可再发行组件包下载:<https://learn.microsoft.com/cpp/windows/latest-supported-vc-redist>",
        "",
    ])


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def default_cargo_homes() -> list[Path]:
    homes: list[Path] = []
    env_home = os.environ.get("CARGO_HOME")
    if env_home:
        homes.append(Path(env_home))
    homes.append(Path.home() / ".cargo")
    return homes


def default_npm_caches() -> list[Path]:
    caches: list[Path] = []
    env_cache = os.environ.get("npm_config_cache") or os.environ.get("NPM_CONFIG_CACHE")
    if env_cache:
        caches.append(Path(env_cache))
    caches.append(Path.home() / ".npm")
    return caches


def build_all_sections(
    repo_root: Path,
    cargo_homes: list[Path],
    npm_cache_roots: list[Path] | None = None,
) -> list[dict]:
    workspace_lock = repo_root / "Cargo.lock"
    desktop_lock = repo_root / "apps" / "desktop" / "src-tauri" / "Cargo.lock"
    npm_lock = repo_root / "apps" / "desktop" / "package-lock.json"
    npm_root = repo_root / "apps" / "desktop" / "node_modules"
    return [
        {
            "scope": SCOPE_WORKSPACE,
            "entries": build_rust_inventory(workspace_lock, repo_root, cargo_homes),
            "lock_path": workspace_lock,
            "lock_label": "Cargo.lock",
        },
        {
            "scope": SCOPE_DESKTOP,
            "entries": build_rust_inventory(desktop_lock, repo_root, cargo_homes),
            "lock_path": desktop_lock,
            "lock_label": "apps/desktop/src-tauri/Cargo.lock",
        },
        {
            "scope": SCOPE_NPM,
            "entries": build_npm_inventory(npm_lock, npm_root, npm_cache_roots),
            "lock_path": npm_lock,
            "lock_label": "apps/desktop/package-lock.json",
        },
    ]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="离线生成第三方许可清单(#56)")
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--output", type=Path, default=None, help="默认 docs/third-party-licenses.md")
    parser.add_argument("--cargo-home", type=Path, action="append", default=None,
                        help="额外 cargo home(可重复);默认 $CARGO_HOME 与 ~/.cargo")
    parser.add_argument("--npm-cache", type=Path, action="append", default=None,
                        help="额外 npm 缓存目录(可重复);默认 $npm_config_cache 与 ~/.npm")
    parser.add_argument("--check", action="store_true", help="校验现有文档与重跑输出一致")
    parser.add_argument("--self-check", action="store_true", help="运行内置自检(不读写仓库文件)")
    args = parser.parse_args(argv)

    if args.self_check:
        return run_self_check()

    repo_root: Path = args.repo_root.resolve()
    cargo_homes = (args.cargo_home or []) and list(args.cargo_home)
    cargo_homes = (cargo_homes or []) + [
        p for p in default_cargo_homes() if p not in cargo_homes
    ]
    npm_caches = (args.npm_cache or []) and list(args.npm_cache)
    npm_caches = (npm_caches or []) + [
        p for p in default_npm_caches() if p not in npm_caches
    ]
    sections = build_all_sections(repo_root, cargo_homes, npm_caches)
    document = render_document(repo_root, sections)

    output = args.output or (repo_root / "docs" / "third-party-licenses.md")
    if args.check:
        existing = output.read_text(encoding="utf-8") if output.is_file() else None
        if existing != document:
            print(f"CHECK FAILED: {output} 与重跑输出不一致", file=sys.stderr)
            return 1
        print(f"CHECK OK: {output} 与重跑输出逐字节一致")
        return 0

    output.write_text(document, encoding="utf-8")
    total = sum(len(s["entries"]) for s in sections)
    unknown = sum(1 for s in sections for e in s["entries"] if e.is_unknown)
    print(f"written {output} ({total} 包,unknown {unknown})")
    return 0


# ---------------------------------------------------------------------------
# 自检
# ---------------------------------------------------------------------------

def _make_crate_tar(cache_dir: Path, name: str, version: str, license_id: str) -> Path:
    import tarfile as tf

    crate_path = cache_dir / f"{name}-{version}.crate"
    inner_name = f"{name}-{version}"
    manifest = f'[package]\nname = "{name}"\nversion = "{version}"\nlicense = "{license_id}"\n'.encode()
    buf = io.BytesIO()
    with tf.open(fileobj=buf, mode="w:gz") as tar:
        info = tf.TarInfo(f"{inner_name}/Cargo.toml")
        info.size = len(manifest)
        tar.addfile(info, io.BytesIO(manifest))
    crate_path.write_bytes(buf.getvalue())
    return crate_path


def _build_fixture(root: Path) -> dict:
    """构造最小仓库夹具:2 个 registry 包 + 1 个成员 crate + 2 个 npm 包。"""
    (root / "docs").mkdir(parents=True, exist_ok=True)
    (root / "scripts").mkdir(parents=True, exist_ok=True)
    (root / "crates" / "wardrobe-core").mkdir(parents=True, exist_ok=True)
    src_root = root / "cargo" / "registry" / "src" / "index.crates.io-test"
    cache_root = root / "cargo" / "registry" / "cache" / "index.crates.io-test"
    src_root.mkdir(parents=True, exist_ok=True)
    cache_root.mkdir(parents=True, exist_ok=True)

    # registry 源码缓存可解析
    (src_root / "demo-lib-1.0.0").mkdir()
    (src_root / "demo-lib-1.0.0" / "Cargo.toml").write_text(
        '[package]\nname = "demo-lib"\nversion = "1.0.0"\nlicense = "MIT OR Apache-2.0"\n',
        encoding="utf-8",
    )
    # 仅 .crate 缓存可解析
    _make_crate_tar(cache_root, "demo-crate-cache", "0.2.3", "BSD-3-Clause")
    # GPL-only 包(应进待审查名单)
    _make_crate_tar(cache_root, "demo-gpl", "9.9.9", "GPL-3.0-only")

    (root / "Cargo.lock").write_text(
        """
[[package]]
name = "wardrobe-core"
version = "0.1.0"

[[package]]
name = "demo-lib"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "demo-crate-cache"
version = "0.2.3"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "demo-gpl"
version = "9.9.9"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "demo-missing"
version = "4.1.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
""",
        encoding="utf-8",
    )
    (root / "crates" / "wardrobe-core" / "Cargo.toml").write_text(
        '[package]\nname = "wardrobe-core"\nversion = "0.1.0"\n', encoding="utf-8"
    )

    # src-tauri 独立 lock(复用 demo-lib)
    (root / "apps" / "desktop" / "src-tauri").mkdir(parents=True, exist_ok=True)
    (root / "apps" / "desktop" / "src-tauri" / "Cargo.lock").write_text(
        """
[[package]]
name = "demo-lib"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
""",
        encoding="utf-8",
    )

    # npm:一个有 license 字段、一个只有 LICENSE 文件、一个仅存在于 npm 缓存 tarball
    nm = root / "apps" / "desktop" / "node_modules"
    (nm / "pkg-a").mkdir(parents=True)
    (nm / "pkg-a" / "package.json").write_text(
        '{"name": "pkg-a", "version": "2.0.0", "license": "(MIT)"}', encoding="utf-8"
    )
    (nm / "pkg-b").mkdir(parents=True)
    (nm / "pkg-b" / "package.json").write_text('{"name": "pkg-b", "version": "3.1.4"}', encoding="utf-8")
    (nm / "pkg-b" / "LICENSE").write_text("dummy", encoding="utf-8")
    (root / "apps" / "desktop" / "package-lock.json").write_text(
        json.dumps(
            {
                "name": "desktop",
                "lockfileVersion": 3,
                "packages": {
                    "": {"name": "desktop"},
                    "node_modules/pkg-a": {"version": "2.0.0"},
                    "node_modules/pkg-b": {"version": "3.1.4"},
                    "node_modules/pkg-c": {
                        "version": "5.0.1",
                        "resolved": "https://registry.npmjs.org/pkg-c/-/pkg-c-5.0.1.tgz",
                    },
                },
            }
        ),
        encoding="utf-8",
    )
    # 模拟 npm cacache:index-v5 记录 + content-v2 存 tarball
    import hashlib as _hashlib

    npm_cache = root / "npm-cache"
    manifest_bytes = json.dumps(
        {"name": "pkg-c", "version": "5.0.1", "license": "Apache-2.0"}
    ).encode()
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tar:
        info = tarfile.TarInfo("package/package.json")
        info.size = len(manifest_bytes)
        tar.addfile(info, io.BytesIO(manifest_bytes))
    blob = buf.getvalue()
    hex_digest = _hashlib.sha512(blob).hexdigest()
    integrity = "sha512-" + base64.b64encode(_hashlib.sha512(blob).digest()).decode()
    (npm_cache / "_cacache" / "index-v5" / hex_digest[:2] / hex_digest[2:4]).mkdir(parents=True)
    index_entry = json.dumps(
        {
            "key": "make-fetch-happen:request-cache:https://registry.npmjs.org/pkg-c/-/pkg-c-5.0.1.tgz",
            "integrity": integrity,
        }
    )
    (npm_cache / "_cacache" / "index-v5" / hex_digest[:2] / hex_digest[2:4] / hex_digest[4:]).write_text(
        f"{hex_digest[:32]}\t{index_entry}\n", encoding="utf-8"
    )
    content = (
        npm_cache / "_cacache" / "content-v2" / "sha512"
        / hex_digest[:2] / hex_digest[2:4] / hex_digest[4:]
    )
    content.parent.mkdir(parents=True)
    content.write_bytes(blob)
    return {"repo_root": root, "cargo_home": root / "cargo", "npm_cache": npm_cache}


def run_self_check() -> int:
    import tempfile
    import traceback

    failures: list[str] = []

    def check(condition: bool, label: str) -> None:
        if condition:
            print(f"  ok: {label}")
        else:
            failures.append(label)
            print(f"  FAIL: {label}")

    try:
        with tempfile.TemporaryDirectory(prefix="license-inventory-selfcheck-") as tmp:
            root = Path(tmp)
            fx = _build_fixture(root)
            sections = build_all_sections(
                fx["repo_root"], [fx["cargo_home"]], [fx["npm_cache"]]
            )
            rust = sections[0]["entries"]
            npm = sections[2]["entries"]

            check(len(rust) == 5, f"Rust 去重计数 == 5(实际 {len(rust)})")
            by_name = {e.name: e for e in rust}
            check(by_name["demo-lib"].license == "MIT OR Apache-2.0", "registry 源码缓存提取 license")
            check(by_name["demo-crate-cache"].license == "BSD-3-Clause", ".crate 缓存提取 license")
            check(by_name["demo-gpl"].license == "GPL-3.0-only", "GPL 包标识读取")
            check(by_name["demo-gpl"].needs_review, "GPL-only 进入待审查")
            check(by_name["demo-missing"].license == UNKNOWN, "缓存缺失标记 unknown")
            check(by_name["demo-missing"].needs_review, "unknown 进入待审查")
            check(by_name["wardrobe-core"].status == STATUS_MEMBER_NO_LICENSE, "自有 crate 无 license 字段被单独标注")
            check(by_name["wardrobe-core"].needs_review is False, "自有 crate 不进第三方待审查")
            check(not by_name["demo-lib"].needs_review, "MIT OR Apache-2.0 不进待审查")

            check(len(npm) == 3, f"npm 计数 == 3(实际 {len(npm)})")
            npm_by_name = {e.name: e for e in npm}
            check(npm_by_name["pkg-a"].license == "MIT", "npm license 字段去括号")
            check(npm_by_name["pkg-b"].license == UNKNOWN, "npm 仅 LICENSE 文件标 unknown")
            check(npm_by_name["pkg-c"].license == "Apache-2.0", "npm 缓存 tarball 提取 license")
            check(npm_by_name["pkg-c"].status == STATUS_NPM_CACHE_TARBALL, "npm 缓存来源状态正确")
            desktop = sections[1]["entries"]
            check(len(desktop) == 1 and desktop[0].name == "demo-lib", "src-tauri lock 独立成节")

            # 表达式分类边界
            check(is_permissive_expression("MIT"), "MIT 宽松")
            check(is_permissive_expression("(MIT OR Apache-2.0) AND ISC"), "组合 AND 宽松")
            check(is_permissive_expression("Apache-2.0 WITH LLVM-exception"), "WITH 例外宽松")
            check(not is_permissive_expression("(MIT OR Apache-2.0) AND Unicode-DFS-2016"),
                  "AND Unicode 分支保守进待审查")
            check(not is_permissive_expression("GPL-2.0-only"), "GPL-only 非宽松")
            check(not is_permissive_expression(UNKNOWN), "unknown 非宽松")

            # 幂等:两次渲染逐字节一致
            doc1 = render_document(fx["repo_root"], sections)
            doc2 = render_document(
                fx["repo_root"],
                build_all_sections(fx["repo_root"], [fx["cargo_home"]], [fx["npm_cache"]]),
            )
            check(doc1 == doc2, "重跑渲染逐字节一致")

            # 输出落盘后 --check 通过
            out_path = fx["repo_root"] / "docs" / "third-party-licenses.md"
            out_path.write_text(doc1, encoding="utf-8")
            check(out_path.read_text(encoding="utf-8") == doc1, "落盘内容与渲染一致")
            check("MSVC CRT" in doc1, "文档含 MSVC CRT 节")
    except Exception:
        failures.append("自检异常")
        traceback.print_exc()

    if failures:
        print(f"self-check FAILED: {len(failures)} 项 -> {failures}", file=sys.stderr)
        return 1
    print("self-check OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
