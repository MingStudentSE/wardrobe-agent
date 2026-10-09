#!/usr/bin/env python3
"""write-provenance.py — 为发行产物目录写 PROVENANCE.json 并复核 SHA256SUMS。

背景:2026-10-09 事故,现装 App 的二进制哈希在所有留存产物里找不到同源记录,
无法溯源到 commit。此后每个发行产物目录必须携带 PROVENANCE.json。

用法:
    python3 scripts/write-provenance.py --dir <产物目录> \
        [--commit <sha>] [--build-command "<命令>"] \
        [--checksums <文件名>] [--repo <仓库根>] [--dry-run] \
        [--require-provenance]

约定:
- --dir 必须显式给出;脚本绝不默认指向任何系统或数据目录。
- 仓库根默认取脚本所在目录的父目录(--repo 可覆盖);source_commit 取
  `git rev-parse HEAD`,dirty 取 `git status --porcelain`。
- 复核并重写 SHA256SUMS.txt(或已存在的 SHA256SUMS),使其与目录内实际
  字节一致;PROVENANCE.json 与校验和文件本身不计入 artifacts。
- --dry-run 只校验不写盘;发现已有条目哈希不符或指向缺失文件时退出码 2。
- --require-provenance 与 --dry-run 搭配:额外校验目录已携带 PROVENANCE.json
  且 source_commit 非空,不满足时同样退出码 2——缺 PROVENANCE 的目录
  不得作为发布候选留存。该旗标只在 dry-run 下有意义(写模式会重建
  PROVENANCE.json),单独使用报参数错误。
- 纯标准库;幂等:重跑整体覆盖更新,不追加。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

PROVENANCE_NAME = "PROVENANCE.json"
CHECKSUM_CANDIDATES = ("SHA256SUMS.txt", "SHA256SUMS")

EXIT_OK = 0
EXIT_ERROR = 1
EXIT_MISMATCH = 2


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run_git(repo: Path, *args: str) -> str:
    proc = subprocess.run(
        ["git", "-C", str(repo), *args],
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        raise RuntimeError(
            f"git -C {repo} {' '.join(args)} 失败:\n{proc.stderr.strip()}"
        )
    return proc.stdout


def collect_artifacts(root: Path, checksums_name: str) -> dict[str, str]:
    """相对 POSIX 路径 → sha256;排除 PROVENANCE.json 与校验和文件本身。"""
    digests: dict[str, str] = {}
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(root).as_posix()
        if rel == PROVENANCE_NAME or rel == checksums_name:
            continue
        digests[rel] = sha256_file(path)
    return digests


def parse_checksums(text: str) -> list[tuple[str, str]]:
    entries: list[tuple[str, str]] = []
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        digest, _, name = line.partition(" ")
        name = name.lstrip(" *")
        entries.append((digest.strip(), name))
    return entries


def render_checksums(digests: dict[str, str]) -> str:
    lines = [f"{digest}  {name}" for name, digest in sorted(digests.items())]
    return "".join(line + "\n" for line in lines)


def verify_entries(
    entries: list[tuple[str, str]], actual: dict[str, str]
) -> list[str]:
    problems: list[str] = []
    for digest, name in entries:
        if name not in actual:
            problems.append(f"条目指向不存在的文件: {name}")
        elif digest != actual[name]:
            problems.append(f"哈希不符: {name} 记录 {digest} 实际 {actual[name]}")
    return problems


def provenance_defect(root: Path) -> Optional[str]:
    """发布候选门槛:PROVENANCE.json 必须存在且 source_commit 非空。

    返回缺陷描述;满足要求时返回 None。
    """
    provenance_path = root / PROVENANCE_NAME
    if not provenance_path.is_file():
        return (f"缺少 {PROVENANCE_NAME}:缺 PROVENANCE 的目录"
                f"不得作为发布候选留存")
    try:
        provenance = json.loads(provenance_path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError) as exc:
        return f"{PROVENANCE_NAME} 无法解析: {exc}"
    if not isinstance(provenance, dict) \
            or not str(provenance.get("source_commit") or "").strip():
        return f"{PROVENANCE_NAME} 的 source_commit 为空:不满足发布候选要求"
    return None


def tool_versions() -> dict[str, Optional[str]]:
    versions: dict[str, Optional[str]] = {"python": None, "node": None, "rustc": None}
    versions["python"] = f"{sys.version_info.major}." \
                         f"{sys.version_info.minor}.{sys.version_info.micro}"
    for key, argv in (("node", ("node", "--version")), ("rustc", ("rustc", "--version"))):
        try:
            proc = subprocess.run(argv, capture_output=True, text=True)
            versions[key] = proc.stdout.strip() or None if proc.returncode == 0 else None
        except OSError:
            versions[key] = None
    return versions


def build_provenance(
    commit: str,
    dirty_files: list[str],
    build_command: str,
    digests: dict[str, str],
) -> dict:
    return {
        "schema_version": 1,
        "source_commit": commit,
        "dirty": bool(dirty_files),
        "dirty_files": dirty_files,
        "build_command": build_command,
        "tool_versions": tool_versions(),
        "created_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "artifacts": digests,
    }


def main(argv: Optional[list[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        description="为发行产物目录写 PROVENANCE.json 并复核 SHA256SUMS。",
    )
    parser.add_argument("--dir", required=True, metavar="<产物目录>",
                        help="发行产物目录(必须显式给出)")
    parser.add_argument("--repo", metavar="<仓库根>",
                        help="源码仓库根;默认取脚本所在目录的父目录")
    parser.add_argument("--commit", metavar="<sha>",
                        help="覆盖记录的源 commit;默认 git rev-parse HEAD")
    parser.add_argument("--build-command", metavar="<命令>",
                        help="产出该批产物的构建命令;默认记录本脚本调用")
    parser.add_argument("--checksums", metavar="<文件名>",
                        help="校验和文件名;默认 SHA256SUMS.txt,兼容已存在的 SHA256SUMS")
    parser.add_argument("--dry-run", action="store_true",
                        help="只校验并报告,不写任何文件")
    parser.add_argument("--require-provenance", action="store_true",
                        help="与 --dry-run 搭配:校验 PROVENANCE.json 存在且 "
                             "source_commit 非空,不满足时退出码 2")
    args = parser.parse_args(argv)
    if args.require_provenance and not args.dry_run:
        parser.error(
            "--require-provenance 只能与 --dry-run 搭配使用"
            "(写模式会重建 PROVENANCE.json,校验无意义)"
        )

    root = Path(args.dir).expanduser().resolve()
    if not root.is_dir():
        print(f"错误: --dir 不是存在的目录: {root}", file=sys.stderr)
        return EXIT_ERROR

    repo = Path(args.repo).expanduser().resolve() if args.repo \
        else Path(__file__).resolve().parents[1]

    try:
        commit = args.commit or run_git(repo, "rev-parse", "HEAD").strip()
        dirty_files = [
            line for line in run_git(repo, "status", "--porcelain").splitlines()
            if line.strip()
        ]
    except RuntimeError as exc:
        print(f"错误: {exc}", file=sys.stderr)
        return EXIT_ERROR

    checksums_name = args.checksums
    if not checksums_name:
        checksums_name = next(
            (name for name in CHECKSUM_CANDIDATES if (root / name).is_file()),
            CHECKSUM_CANDIDATES[0],
        )
    checksums_path = root / checksums_name

    digests = collect_artifacts(root, checksums_name)

    existing_text: Optional[str] = None
    if checksums_path.is_file():
        existing_text = checksums_path.read_text(encoding="utf-8")
    problems = verify_entries(parse_checksums(existing_text or ""), digests)

    new_text = render_checksums(digests)

    defect = provenance_defect(root) if args.require_provenance else None

    if args.dry_run:
        if existing_text is None:
            print(f"[dry-run] 将创建 {checksums_path.name}({len(digests)} 个条目)")
        elif problems:
            print(f"[dry-run] {checksums_path.name} 校验发现问题。", file=sys.stderr)
        else:
            print(f"[dry-run] {checksums_path.name} 已有条目与实际字节一致。")
            if new_text != existing_text:
                print(f"[dry-run] 将补全条目至 {len(digests)} 个文件。")
        for problem in problems:
            print(f"[dry-run] {problem}", file=sys.stderr)
        print(f"[dry-run] PROVENANCE.json: source_commit={commit} "
              f"dirty={bool(dirty_files)} artifacts={len(digests)}")
        if args.require_provenance:
            if defect:
                print(f"[dry-run] {defect}", file=sys.stderr)
            else:
                print("[dry-run] PROVENANCE.json 存在且 source_commit 非空。")
        return EXIT_MISMATCH if (problems or defect) else EXIT_OK

    if existing_text != new_text:
        checksums_path.write_text(new_text, encoding="utf-8")
        action = "创建" if existing_text is None else "更新"
        print(f"{action} {checksums_path.name}({len(digests)} 个条目)")
    else:
        print(f"{checksums_path.name} 已与实际字节一致,无需改动。")
    for problem in problems:
        print(f"警告(已修正): {problem}", file=sys.stderr)

    provenance = build_provenance(
        commit=commit,
        dirty_files=dirty_files,
        build_command=args.build_command or " ".join(sys.argv),
        digests=digests,
    )
    provenance_path = root / PROVENANCE_NAME
    provenance_path.write_text(
        json.dumps(provenance, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(f"写入 {provenance_path}")
    return EXIT_OK


if __name__ == "__main__":
    sys.exit(main())
