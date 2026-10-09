#!/usr/bin/env python3
"""generate-license-inventory.py 的最小自检桥接(issue #56)。

- pytest 可用:`pytest scripts/tests/`
- 无 pytest:直接 `python3 scripts/tests/test_generate_license_inventory.py`
自检逻辑本体在 scripts/generate-license-inventory.py 的 run_self_check(),
使用临时目录夹具,不读写仓库文件。
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

_SCRIPT = Path(__file__).resolve().parent.parent / "generate-license-inventory.py"


def _load_module():
    spec = importlib.util.spec_from_file_location("generate-license-inventory", _SCRIPT)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def test_self_check() -> None:
    module = _load_module()
    assert module.run_self_check() == 0, "内置自检未全部通过"


if __name__ == "__main__":
    test_self_check()
    print("test_generate_license_inventory: PASS")
