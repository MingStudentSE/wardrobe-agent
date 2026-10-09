#!/usr/bin/env python3
"""write-provenance.py 的行为测试(issue #61)。

seam:CLI 公共接口——`python3 scripts/write-provenance.py --dir <产物目录>`,
观察点是目录里落盘的 PROVENANCE.json 与 SHA256SUMS(.txt) 的实际字节。

运行:python3 scripts/tests/test_write_provenance.py
  或:python3 -m unittest discover -s scripts/tests -t .
  或:pytest scripts/tests/test_write_provenance.py
"""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from datetime import datetime, timedelta
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT = REPO_ROOT / "scripts" / "write-provenance.py"

BIN_NAME = "wardrobe-cli.tar.gz"
BIN_BYTES = b"\xfa\xce\x00\x01wardrobe-fake-binary-payload\n"
NESTED_REL = "docs/note.txt"
NESTED_BYTES = "电子衣橱 fake artifact note\n".encode("utf-8")


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _git(repo: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo), *args],
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout


def _make_git_repo(base: Path) -> Path:
    repo = base / "src-repo"
    repo.mkdir()
    _git(repo, "init", "-q", "-b", "main")
    (repo / "README.md").write_text("hello\n", encoding="utf-8")
    _git(repo, "add", "-A")
    _git(repo, "-c", "user.name=t", "-c", "user.email=t@example.com",
         "commit", "-qm", "init")
    return repo


def _make_artifacts(base: Path) -> Path:
    art = base / "dist"
    (art / "docs").mkdir(parents=True)
    (art / BIN_NAME).write_bytes(BIN_BYTES)
    (art / NESTED_REL).write_bytes(NESTED_BYTES)
    return art


def _run(script_args: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(SCRIPT), *script_args],
        capture_output=True,
        text=True,
    )


class WriteProvenanceTestCase(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory(prefix="provenance-test-")
        self.base = Path(self._tmp.name)
        self.repo = _make_git_repo(self.base)
        self.art = _make_artifacts(self.base)
        self.addCleanup(self._tmp.cleanup)

    def head(self) -> str:
        return _git(self.repo, "rev-parse", "HEAD").strip()

    def run_script(self, *extra: str) -> subprocess.CompletedProcess[str]:
        return _run([
            "--dir", str(self.art),
            "--repo", str(self.repo),
            *extra,
        ])

    def read_provenance(self) -> dict:
        return json.loads(
            (self.art / "PROVENANCE.json").read_text(encoding="utf-8")
        )


class TestProvenanceFields(WriteProvenanceTestCase):
    """票面字段:source_commit/dirty/build_command/tool_versions/created_at/artifacts。"""

    def test_run_writes_provenance_with_all_fields(self) -> None:
        proc = self.run_script("--build-command", "cargo build --release")
        self.assertEqual(proc.returncode, 0, proc.stderr)

        provenance = self.read_provenance()
        self.assertEqual(provenance["source_commit"], self.head())
        self.assertIs(provenance["dirty"], False)
        self.assertEqual(provenance["dirty_files"], [])
        self.assertEqual(provenance["build_command"], "cargo build --release")

        versions = provenance["tool_versions"]
        self.assertIn("rustc", versions)
        self.assertIn("node", versions)
        self.assertIn("python", versions)

        created = datetime.fromisoformat(provenance["created_at"])
        self.assertIsNotNone(created.utcoffset())
        self.assertEqual(created.utcoffset(), timedelta(0))

        self.assertEqual(
            provenance["artifacts"],
            {
                BIN_NAME: _sha256(BIN_BYTES),
                NESTED_REL: _sha256(NESTED_BYTES),
            },
        )
        # PROVENANCE 自身与校验和文件不进入 artifacts。
        self.assertNotIn("PROVENANCE.json", provenance["artifacts"])
        self.assertNotIn("SHA256SUMS.txt", provenance["artifacts"])

    def test_commit_override_is_recorded_verbatim(self) -> None:
        sha = "0123456789abcdef0123456789abcdef01234567"
        proc = self.run_script("--commit", sha)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(self.read_provenance()["source_commit"], sha)


class TestDirtyStatus(WriteProvenanceTestCase):
    def test_uncommitted_change_is_reported(self) -> None:
        (self.repo / "README.md").write_text("hello\nworld\n", encoding="utf-8")
        proc = self.run_script()
        self.assertEqual(proc.returncode, 0, proc.stderr)

        provenance = self.read_provenance()
        self.assertIs(provenance["dirty"], True)
        self.assertTrue(
            any("README.md" in line for line in provenance["dirty_files"]),
            provenance["dirty_files"],
        )


class TestChecksums(WriteProvenanceTestCase):
    def test_checksums_file_created_and_matches_bytes(self) -> None:
        proc = self.run_script()
        self.assertEqual(proc.returncode, 0, proc.stderr)

        sums_path = self.art / "SHA256SUMS.txt"
        self.assertTrue(sums_path.is_file())
        # 条目按相对路径字典序排列。
        lines = sums_path.read_text(encoding="utf-8").splitlines()
        self.assertEqual(
            lines,
            [
                f"{_sha256(NESTED_BYTES)}  {NESTED_REL}",
                f"{_sha256(BIN_BYTES)}  {BIN_NAME}",
            ],
        )

    def test_wrong_and_stale_entries_are_rewritten(self) -> None:
        sums_path = self.art / "SHA256SUMS.txt"
        sums_path.write_text(
            f"{'0' * 64}  {BIN_NAME}\n"          # 错哈希
            f"{_sha256(NESTED_BYTES)}  {NESTED_REL}\n"
            f"{'f' * 64}  ghost.bin\n",          # 已不存在的文件
            encoding="utf-8",
        )
        proc = self.run_script()
        self.assertEqual(proc.returncode, 0, proc.stderr)

        lines = sums_path.read_text(encoding="utf-8").splitlines()
        self.assertEqual(
            lines,
            [
                f"{_sha256(NESTED_BYTES)}  {NESTED_REL}",
                f"{_sha256(BIN_BYTES)}  {BIN_NAME}",
            ],
        )

    def test_dry_run_reports_mismatch_without_writing(self) -> None:
        sums_path = self.art / "SHA256SUMS.txt"
        bad = f"{'0' * 64}  {BIN_NAME}\n"
        sums_path.write_text(bad, encoding="utf-8")

        proc = self.run_script("--dry-run")
        self.assertEqual(proc.returncode, 2, proc.stdout + proc.stderr)
        self.assertEqual(sums_path.read_text(encoding="utf-8"), bad)
        self.assertFalse((self.art / "PROVENANCE.json").exists())

    def test_existing_sha256sums_file_is_updated_in_place(self) -> None:
        sums_path = self.art / "SHA256SUMS"
        sums_path.write_text(
            f"{_sha256(BIN_BYTES)}  {BIN_NAME}\n", encoding="utf-8"
        )
        proc = self.run_script()
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertTrue(sums_path.is_file())
        self.assertFalse((self.art / "SHA256SUMS.txt").exists())
        self.assertIn(f"{_sha256(NESTED_BYTES)}  {NESTED_REL}",
                      sums_path.read_text(encoding="utf-8"))


class TestIdempotency(WriteProvenanceTestCase):
    def test_rerewrite_is_update_not_append(self) -> None:
        first = self.run_script()
        self.assertEqual(first.returncode, 0, first.stderr)
        sums_after_first = (self.art / "SHA256SUMS.txt").read_bytes()

        second = self.run_script()
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(
            (self.art / "SHA256SUMS.txt").read_bytes(), sums_after_first
        )

        provenance = self.read_provenance()  # 仍是合法单个 JSON 对象
        self.assertEqual(
            sorted(provenance["artifacts"]), sorted([BIN_NAME, NESTED_REL])
        )


class TestRequireProvenance(WriteProvenanceTestCase):
    """--require-provenance:dry-run 额外把「缺 PROVENANCE 的目录不得作为发布候选」
    落成机器断言——PROVENANCE.json 缺失或 source_commit 为空时非零退出。"""

    def test_dry_run_fails_when_provenance_missing(self) -> None:
        proc = self.run_script("--dry-run", "--require-provenance")
        self.assertEqual(proc.returncode, 2, proc.stdout + proc.stderr)
        self.assertIn("PROVENANCE", proc.stderr)
        # 纯校验:不得顺手补写任何文件
        self.assertFalse((self.art / "PROVENANCE.json").exists())

    def test_dry_run_fails_when_source_commit_empty(self) -> None:
        (self.art / "PROVENANCE.json").write_text(
            json.dumps({"schema_version": 1, "source_commit": ""}),
            encoding="utf-8",
        )
        proc = self.run_script("--dry-run", "--require-provenance")
        self.assertEqual(proc.returncode, 2, proc.stdout + proc.stderr)
        self.assertIn("source_commit", proc.stderr)

    def test_dry_run_fails_when_provenance_unparseable(self) -> None:
        (self.art / "PROVENANCE.json").write_text("not json", encoding="utf-8")
        proc = self.run_script("--dry-run", "--require-provenance")
        self.assertEqual(proc.returncode, 2, proc.stdout + proc.stderr)

    def test_dry_run_passes_when_provenance_present_with_commit(self) -> None:
        first = self.run_script()  # 正常模式写入合法 PROVENANCE.json
        self.assertEqual(first.returncode, 0, first.stderr)
        proc = self.run_script("--dry-run", "--require-provenance")
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("PROVENANCE.json", proc.stdout)

    def test_require_provenance_needs_dry_run(self) -> None:
        # 写模式会重建 PROVENANCE.json,该旗标只在 dry-run 校验下有意义;
        # 单独使用必须显式报错,不得被静默忽略。
        proc = self.run_script("--require-provenance")
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn("--require-provenance", proc.stderr)


class TestCliContract(WriteProvenanceTestCase):
    def test_dir_argument_is_required(self) -> None:
        proc = _run(["--repo", str(self.repo)])
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn("--dir", proc.stderr)

    def test_missing_dir_fails_without_creating_anything(self) -> None:
        missing = self.base / "no-such-dist"
        proc = _run(["--dir", str(missing), "--repo", str(self.repo)])
        self.assertNotEqual(proc.returncode, 0)
        self.assertFalse(missing.exists())


if __name__ == "__main__":
    unittest.main()
