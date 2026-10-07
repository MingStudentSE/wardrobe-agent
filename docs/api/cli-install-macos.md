# wardrobe-cli macOS 独立安装（#33 4.4）

## 产物

自包含目录（无源码、无 Rust 工具链、无 GUI 依赖）：

```
wardrobe-cli/
  bin/wardrobe-cli      # 主执行文件，LC_RPATH=@executable_path/../lib，ad-hoc 签名
  lib/libheif.1.dylib   # 原生 HEIF/HEIC 解码（Apache-2.0）
  lib/libde265.0.dylib  # libheif 的 HEVC 解码依赖（LGPL-3.0，动态链接未修改分发）
```

已测架构与系统：macOS Apple Silicon (arm64)，实测本机 macOS 26（2026-10）。最低系统按 Swift 运行库随系统提供为准；x64 构建未在本轮实测，发行前须补充。

## 安装步骤（备份盘临时 prefix 示例）

```sh
# 1. 解包/复制 wardrobe-cli/ 目录到目标前缀（示例为备份盘临时目录）
cp -R wardrobe-cli /path/to/prefix/
# 2. 加入 PATH（shell 配置或每次调用）
export PATH="/path/to/prefix/bin:$PATH"
# 3. 验证
wardrobe-cli --version
wardrobe-cli --help
```

无需 `DYLD_LIBRARY_PATH`（二进制经 `@executable_path/../lib` 自定位）。替换或升级时覆盖 `bin/wardrobe-cli` 后重新 ad-hoc/正式签名即可；数据库、照片、授权与 profile 配置均不受影响。

## 数据与配置位置

- 数据库：默认 `~/Library/Application Support/wardrobe-agent/wardrobe.sqlite`；可用 `--db`、`--profile`（`wardrobe-cli profile.add <名称> --data-dir <绝对路径>`）或 `WARDROBE_DATA_DIR` 指定。
- profile 配置：`~/Library/Application Support/wardrobe-agent/profiles.json`（仅存数据目录映射，非秘密）。
- `wardrobe-cli profile.remove <名称>` 只删配置条目，绝不删除数据目录；卸载删除安装目录同样不影响用户数据。

## 已验证范围（2026-10-07，备份盘临时 prefix）

- `env -i`（无 `DYLD_LIBRARY_PATH`）、最小 PATH、离开源码目录调用：`--version`/`--help` 成功。
- profile.add + `--profile` 真实 SQLite 建空间/成员/单品/列表读写。
- 真实 HEIC 解码：`assets.import` 导入 .heic（libheif+libde265）→ 采用压缩展示图成功。
- 升级：覆盖二进制后数据完整保留；卸载：删除安装目录后 `wardrobe.sqlite` 完好。
- 失败恢复：安装中断时重新解包整个目录即可；数据库损坏走 `backup.restore`（先 `backup.preview`）。

## 限制

- 未签名/未公证：Gatekeeper 首次运行可能需右键打开或 `xattr -d com.apple.quarantine`；正式发行签名公证归 #35。
- libde265 为 LGPL 动态链接，随包分发未做修改；许可清单在 #35 汇总。
