# 跨平台架构审查（issue #35，macOS-only 发行轮）

审查基线：分支 `codex/ticket-35-release`（起点 = `codex/beta-integration-continue` tip `464a845`）。
审查日期：2026-10-07。范围依据 ticket #35「当前发行范围」：本轮仅发行与验收 macOS；Windows/Linux 安装包与真实客户端运行延后，不阻塞本轮 Beta；共享核心保持跨平台。

## 结论

共享 Rust Core/Application 无 macOS 专属依赖；全部 OS 集成位于适配边界。逐项证据如下。

## 1. 共享业务核心的平台条件编译清点

`crates/wardrobe-core/src/` 全目录 `#[cfg(target_os = ...)]` 清点结果：**仅 4 处，全部是文件系统路径适配**，无任何业务逻辑分支：

| 位置 | 用途 |
| --- | --- |
| `src/lib.rs:535` | `default_database_path()` macOS 分支：`~/Library/Application Support/wardrobe-agent/wardrobe.sqlite` |
| `src/lib.rs:537` | 同函数 Windows 分支：`%APPDATA%/wardrobe-agent/wardrobe.sqlite` |
| `src/profiles.rs:38` | `profiles_path()` macOS 分支：`~/Library/Application Support/wardrobe-agent/profiles.json` |
| `src/profiles.rs:40` | 同函数 Windows 分支：`%APPDATA%/wardrobe-agent/profiles.json` |

两个函数都有 Unix（含 Linux XDG）分支与 `WARDROBE_DATA_DIR` 环境变量覆盖（`lib.rs:528`），三平台路径语义等价。

`crates/wardrobe-cli/src/`：**0 处** target_os 条件编译。CLI/MCP 完全可移植。
macOS 专属符号（objc/CoreFoundation/AppKit 等）在 core 与 cli 中均无直接引用；`libheif-rs/libheif-sys` 是跨平台 C 绑定（各平台链接各自的 libheif 构建）。

## 2. 平台适配边界逐项核对

| OS 集成 | 边界位置 | 证据 |
| --- | --- | --- |
| 文件路径 | `wardrobe_core::default_database_path()` / `profiles_path()`（见上） | 唯一路径分支点；GUI/CLI/MCP 一律经此函数取路径，不自行拼路径 |
| 安全凭据 | `keyring` crate（`features = ["apple-native","windows-native","sync-secret-service","crypto-rust"]`，`crates/wardrobe-core/Cargo.toml`）——macOS Keychain / Windows Credential Manager / Linux Secret Service 在 crate 内适配 | `src/encryption.rs:212`（服务 `wardrobe-agent.encryption`）、`src/providers.rs:45`（模型供应商密钥）、`src/backup.rs:1254`（备份密钥清理）；core 代码只调 `keyring::Entry`，无平台分支 |
| 系统通知 | GUI 壳层 `apps/desktop/src-tauri/src/scheduler.rs:118-160`，经 `notify-rust`，`#[cfg(target_os=...)]` 仅设置平台注册方式（macOS `set_application`/Windows `app_id`/Linux `appname`）；业务事实由 core 的 `schedule_claim_notification` / `schedule_finish_notification` 持久化，壳层失败时把 `failed` 状态写回 core，不丢业务事件 | 通知权限状态如实上报 `managed_by_os_not_queryable` |
| 自启动 | GUI 壳层 `tauri-plugin-autostart`（macOS LaunchAgent），`scheduler.rs:5,12-24`；core 不感知自启动 | `enable/disable_autostart` 命令，错误透出为 `autostart_error` |
| 托盘/窗口 | GUI 壳层 `scheduler.rs::setup`（tauri Tray/Menu API） | 与 core 无耦合 |

GUI 壳层极薄：`apps/desktop/src-tauri/src/main.rs` 仅 3 个 Tauri 命令——`application`（转发到 `wardrobe_core::Application::open(default_database_path()).execute()`）、`search_weather_city`（天气地理编码）、`scheduler::native_schedule`。无业务逻辑在壳层。

## 3. 三入口共用 Application API 与版本化 schema

- **同一执行入口**：GUI（`main.rs::application`）、CLI（`crates/wardrobe-cli/src/main.rs:53`）、MCP（`main.rs:125,173` 与 `mcp()`）都调用 `wardrobe_core::Application::open(...).execute(operation, input)`；操作描述符来自同一注册源 `wardrobe_core::public_descriptors()`（CLI capabilities 与 MCP tools/list 同源，scope 元数据由 `authorization::scope_metadata` 派生，`lib.rs:519-523`）。
- **版本化 schema**：`DATABASE_SCHEMA_VERSION = 23`（`lib.rs:45`）；`Application::open` 打开时读 `PRAGMA user_version`：
  - `version > 23` → 拒绝打开，错误码 `unsupported_schema`（防止旧应用损坏新库，`lib.rs:99-103`）；
  - `version < 23` → 按迁移梯子逐级升级（`lib.rs:142-154` 起调用 members/assets/outfits/wears/insights/care/recommendations/weather/schedules 等各域 `migrate`，并在 `upgrade_branch_shapes` 后修复 learning 触发器——历史缺陷 e465d8d 的修复已在线内）；
  - capabilities 输出自带 `schema_version: 1`（工具契约版本）。
- 迁移在 Immediate 事务内执行；跨进程并发由 SQLite busy_timeout=5s + WAL 约束（`lib.rs:92-93`）。

## 4. 原生媒体运行时（发行包维度）

- libheif 1.23.1 / libde265 1.1.3 以**动态库**随包分发（`scripts/prepare-media-distribution.py` macOS 分支：复制 `libheif.1.dylib`/`libde265.0.dylib` 进 `Contents/Frameworks`，`@loader_path` rpath + ad-hoc 签名）；独立 CLI 以 `@executable_path/lib` rpath 同样处理。
- Rust 侧通过 `libheif-rs 3.0.0`（绑定）跨平台调用；Linux/Windows 分支在同一脚本内有对等实现（`$ORIGIN` rpath / app-local DLL），本轮不构建但结构保留。
- U2NetP 模型不在包内：运行时按固定 URL + SHA256 下载校验（`third-party/U-2-Net-NOTICE.md`）。

## 5. Windows/Linux 延后项清单（本轮不验收，如实列出）

1. Windows NSIS / Linux deb+AppImage 安装包的**当前源码**构建与真实安装运行（历史 CI 证据停留在 `9070fa7`/`7d783db` 旧源码，且 CI 因账户付费阻断停摆——`docs/verification/integration-ci-account-block.md`）。
2. Windows/Linux 的通知、自启动、凭据存储真实平台验证（代码路径存在，无实机证据）。
3. Windows app-local MSVC CRT 分发核验；Linux GUI loader `ldd` 私有解码库核验。
4. 三平台最低支持系统实测（历史声明：Win11 x64、macOS 13+、Ubuntu 22.04/24.04；本轮仅 macOS 13+ 声明进入构建与实测）。

## 6. 与发行决策的关系

本审查证明「跨平台架构保持」这一验收条件（ticket #35 最后一条 criteria）；它**不**证明 Windows/Linux 已构建或已验收——两者按用户 2026-10-07 决策延后，不阻塞 macOS Beta。
