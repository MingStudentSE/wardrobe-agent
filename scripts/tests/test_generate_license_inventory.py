#!/usr/bin/env python3
"""generate-license-inventory.py 的行为测试(issue #56)。

- pytest 可用:`pytest scripts/tests/`
- 无 pytest:`python3 -m unittest discover -s scripts/tests -t .`
  或直接 `python3 scripts/tests/test_generate_license_inventory.py`
自检逻辑本体在 scripts/generate-license-inventory.py 的 run_self_check(),
使用临时目录夹具,不读写仓库文件;输出稳定性验收(重跑逐字节一致)以
CLI 级两次生成 + `--check` 的机器断言落在这里。
"""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

_SCRIPT = Path(__file__).resolve().parent.parent / "generate-license-inventory.py"


def _load_module():
    spec = importlib.util.spec_from_file_location("generate-license-inventory", _SCRIPT)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class TestLicenseInventorySelfCheck(unittest.TestCase):
    def test_self_check_passes(self) -> None:
        module = _load_module()
        assert module.run_self_check() == 0, "内置自检未全部通过"


class TestRerunOutputStable(unittest.TestCase):
    """#56 验收「脚本重跑输出一致」:两次完整 CLI 运行落盘逐字节一致,
    第三次以 --check 复核。"""

    def test_two_runs_produce_byte_identical_output(self) -> None:
        module = _load_module()
        with tempfile.TemporaryDirectory(prefix="license-inventory-rerun-") as tmp:
            fx = module._build_fixture(Path(tmp))
            output = fx["repo_root"] / "docs" / "third-party-licenses.md"
            argv = [
                "--repo-root", str(fx["repo_root"]),
                "--output", str(output),
                "--cargo-home", str(fx["cargo_home"]),
                "--npm-cache", str(fx["npm_cache"]),
            ]
            run = [sys.executable, str(_SCRIPT), *argv]
            first = subprocess.run(run, capture_output=True, text=True)
            self.assertEqual(first.returncode, 0, first.stderr)
            first_bytes = output.read_bytes()

            second = subprocess.run(run, capture_output=True, text=True)
            self.assertEqual(second.returncode, 0, second.stderr)
            self.assertEqual(
                output.read_bytes(),
                first_bytes,
                "两次运行输出必须逐字节一致",
            )

            check = subprocess.run([*run, "--check"], capture_output=True, text=True)
            self.assertEqual(check.returncode, 0, check.stderr)
            self.assertIn("CHECK OK", check.stdout)


if __name__ == "__main__":
    unittest.main()
