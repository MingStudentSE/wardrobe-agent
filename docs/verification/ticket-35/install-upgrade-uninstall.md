# macOS 安装 / 升级 / 卸载实测（issue #35）

日期：2026-10-07/08。机器：Mac16,10（Apple M4，10 核，16GB），macOS 26（darwin 25.6.0）。
产物：`电子衣橱_0.1.0_aarch64.dmg`（SHA256 `aaf81895459c19a25a325f89702d1ef610723cfede842fc6367b5675b98c1fc6`），arm64，最低系统 macOS 13.0（`MACOSX_DEPLOYMENT_TARGET=13.0` 构建，未在旧系统实测，见已知限制）。

## 0. 隔离方案（重要前提）

**验证开始前发现 `/Applications/电子衣橱.app` 已存在用户真实安装且正在运行（进程 wardrobe-desktop），`~/Library/Application Support/wardrobe-agent/` 为用户真实数据。** 按铁律绝不覆盖/删除/触碰，全部实测改用隔离方案：

- 应用安装到隔离目录 `results/continue-20261007/ticket35/install-test/Applications/`（经 DMG 挂载 `ditto` 复制，等效拖拽安装）。
- 数据目录经 `WARDROBE_DATA_DIR` 指向隔离目录 `install-test/data-clean/`、`install-test/data-upgrade/`（`wardrobe_core::default_database_path()` 支持该环境变量，`lib.rs:528`）。
- 所有 CLI 调用一律 `env -i` + 隔离 `HOME`。
- 事件记录：一次旧版 CLI 冒烟在隔离 HOME 建立前误以默认路径打开用户真实库并因 schema 校验立即退出（`unsupported_schema`）；只读打开 + 读取版本号，未进入写事务，用户主库文件 mtime（02:56）早于该事件（05:12）未变化，目录内临时 journal 文件随连接关闭清除。用户数据未受影响；此后全部调用强制隔离 HOME。

原生窗口截图说明：验证会话期间屏幕处于锁定状态（loginwindow 持有全屏窗口），原生窗口级截屏（screencapture -l）被系统拒绝。GUI 启动证据以 CGWindowList 窗口记录（窗口 ID + 尺寸）+ 隔离数据目录建库行为留档；视觉状态截图经同一前端构建（apps/desktop/dist，即 DMG 内打包的同一 bundle）由 wardrobe-web 服务 + 真实 Chrome 渲染获得，并在文件名与文中如实标注。

## 1. 干净安装与首启动

1. `hdiutil attach -readonly -nobrowse` 挂载 DMG，卷内含 `电子衣橱.app` 与指向 `/Applications` 的链接（标准拖拽布局）。
2. `ditto` 复制到隔离安装目录；`codesign --verify --deep --strict` 通过（ad-hoc 签名，未公证）。
3. 首启动（隔离数据目录）：进程 45716 创建主窗口（CGWindowList 记录 window_id=11423，1280×820，与 tauri.conf 一致）；隔离目录出现 `wardrobe.sqlite`，`user_version=23`，默认成员自动创建（members=1）。
4. CLI（同发行包 sidecar）向该库写入「安装验收·白色衬衫」成功，读回一致——GUI 建库与 CLI 共库互通。
5. 视觉留档：`screenshots/02-clean-install-overview.png`、`02-clean-install-inventory.png`（真实 Chrome 渲染同一前端构建，UI 文本检查通过：`logs/02-clean-install-ui-text-check.txt`）。

## 2. 升级（旧版本数据 → 新版本应用）

1. 旧版本基线：使用上一发行候选的独立 CLI（`results/mvp-macos-candidate/wardrobe-cli-aarch64-apple-darwin.tar.gz`，对应集成源码 b075fdd 时期）在 `data-upgrade/` 建库：`user_version=20`，创建 3 件已知单品「升级前衬衫 1/2/3」（`logs/old-schema-state.txt`）。
2. 用新版 .app 启动指向该数据目录：主窗口创建（window_id=11501，1280×820）。
3. 迁移结果：`user_version` 20 → **23**（真实迁移梯子执行，包含 schema21/22/23 分支形状重建与 learning 触发器修复路径）；3 件已知单品逐名核对完好；迁移后经新 CLI 新增「升级后新增外套」成功（`logs/post-upgrade-state.txt`）。
4. 视觉留档：`screenshots/03-upgrade-overview.png`、`03-upgrade-inventory.png`——UI 显示「4 件单品」，升级前 3 件与升级后新增 1 件全部可见（`logs/03-upgrade-ui-text-check.txt`）。
5. 结论：旧库升级数据完好、可继续使用；未出现「数据库由更新版本创建」错误（反向防护已由 CLI 层历史证据覆盖）。

## 3. 卸载与数据保留

1. 删除隔离安装目录中的 `.app`（等效拖入废纸篓）：`rm -rf` 成功。
2. 两个数据目录原样保留：`data-clean/`、`data-upgrade/` 的 `wardrobe.sqlite`（含 WAL/SHM）与资产全部存在；升级库 `user_version=23`、4 件单品完好（`logs/post-uninstall-data-check.txt`）。
3. **数据保留语义（如实）**：卸载应用不删除用户数据。真实用户卸载（拖拽 /Applications/电子衣橱.app 到废纸篓）后仍会保留：
   - `~/Library/Application Support/wardrobe-agent/`（wardrobe.sqlite、assets 原图、profiles.json）；
   - macOS 钥匙串中 `wardrobe-agent.encryption` 服务条目与已配置的模型供应商凭据条目（`keyring`，见 architecture-review.md 第 2 节）。
   恢复使用：重新安装同版本或更高版本即原样继续；如需彻底清除须手动删除上述数据目录与钥匙串条目（文档在发布说明中向用户说明）。
4. 用户真实安装全程未受影响：`/Applications/电子衣橱.app` 仍在且进程持续运行。

## 4. 签名 / 公证 / 依赖状态

| 项 | 状态 |
| --- | --- |
| .app 签名 | ad-hoc（`codesign --force --deep --sign -`；打包器 linker-signed 签名与 overlay 资源不一致问题已显式重签修复，`codesign --verify --deep --strict` 通过） |
| 公证 | **未公证**（无 Apple Developer 证书）；首次打开需右键打开或系统设置放行 |
| spctl 评估 | `spctl --assess --type execute` 在本机通过（退出 0）；该结果不代表有公证票据 |
| DMG 生成 | tauri bundle_dmg.sh `--skip-jenkins`（Finder AppleScript 美化在无头/锁屏会话超时 -1712，跳过后为无自定义图标布局的 DMG，功能等价） |
| 最低系统 | 构建目标 13.0；实测系统为 macOS 26（darwin 25.6.0），13.x 真机未测 |

原始日志：`install-test/logs/`；安装复制与签名核验：`build-release.log`、`bundle-dmg-final.log`。
