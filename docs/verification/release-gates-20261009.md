# 发行门槛三态记录与豁免台账（2026-10-09）

记录 MVP 安装包（issue [#37](https://github.com/MingStudentSE/wardrobe-agent/issues/37)，属 #54 收尾）在 2026-10-09 时点的发行门槛现状。每条门槛标注「满足 / 豁免 / 缺失」三态之一并给出出处。当前应用版本 0.1.0，为研发试用候选，不表示正式发行；完整 Beta 发行验收由 issue [#35](https://github.com/MingStudentSE/wardrobe-agent/issues/35) 独立跟踪，本台账不代其宣称 F01–F30 完成。

外部事实来源（只读产物，不入库）：`wardrobe-execution/results/continue-20261007/mvp-release-20261007.md`、`wardrobe-execution/results/mvp-release-platform-audit.md`、`wardrobe-execution/results/incident-schema-migration-20261008.md`；`wardrobe-execution/` 位于 `/Users/links/.codex/external-workspace-volume/codex/`（见 `docs/agents/build-storage.md`）。

## 门槛总览

| 门槛 | 平台 | 三态 | 出处 |
| --- | --- | --- | --- |
| 代码签名与公证 | macOS | 缺失（已知限制） | mvp-release-20261007.md 构建目标表 |
| Authenticode 签名 | Windows | 缺失（已知限制） | 同上 |
| 发行签名（仓库/GPG） | Linux | 缺失（已知限制） | 同上 |
| macOS 13 最低版本真机验收 | macOS | 豁免（待验） | 同上「构建最低13.0，13真机待验」 |
| 经 GUI 的备份恢复（安装包边界） | macOS | 豁免（当时待验） | 同上「全部使用临时数据库，GUI恢复仍待验」 |
| 安装/运行验收 | Windows | 豁免（用户决策 2026-10-07） | mvp-release-20261007.md「用户最新调整」 |
| 打包构建与包内运行时验证 | Linux | 满足（限定范围） | `docs/verification/packaged-ubuntu-22.04/report.md`、`docs/verification/packaged-ubuntu-24.04/report.md` |
| 真实供应商图像验收（#14 商品图/#20 搭配图/#21 试穿） | 三入口共通 | 满足（2026-10-08 真实云调用） | `docs/verification/ticket-14/report-real-cloud.md` 等，见第 4 节 |
| 逐平台干净安装/升级/卸载保留/发布前门槛 | 三平台 | 缺失（开放） | mvp-release-20261007.md「发布前仍须满足」 |

## 1. 签名现状（三平台）

### macOS：无 Developer ID / 公证 — 缺失（已知限制）

- 现状：全部产物临时（ad-hoc）签名；仓库无任何签名配置，构建机 codesign 可用身份为 0（审计结论 3）。
- 用户可见影响：Gatekeeper 阻止未公证应用首次启动，需右键打开或在「系统设置 → 隐私与安全性」放行；分发时必须如实披露签名状态。
- 解除条件：加入 Apple Developer Program，改用 Developer ID 证书签名，并通过 notarytool 公证 + staple 后分发。
- 出处：`wardrobe-execution/results/continue-20261007/mvp-release-20261007.md` 构建目标表「无Developer ID/公证；临时签名」；`wardrobe-execution/results/mvp-release-platform-audit.md`。

### Windows：Authenticode 未配置 — 缺失（已知限制）

- 现状：NSIS EXE 未做 Authenticode 代码签名，仓库无证书配置。
- 用户可见影响：首次运行触发 SmartScreen「已保护你的电脑」/未知发布者提示，用户需手动选择「仍要运行」。
- 解除条件：购买 OV/EV 代码签名证书并配置 Tauri Windows bundle 签名参数后重新出包。
- 出处：同上构建目标表「Authenticode未配置」。

### Linux：无发行签名 — 缺失（已知限制）

- 现状：DEB 与 AppImage 均无仓库签名或 GPG 签名。
- 用户可见影响：本地安装影响有限；无法经签名 APT 仓库或 GPG 校验确认产物来源，下载渠道需配合发布 SHA256 核对。
- 解除条件：建立 GPG 签名的 APT 仓库，或对 AppImage 提供 GPG 签名与校验说明。
- 出处：同上构建目标表「无发行签名」。

## 2. 逐平台验收记录与豁免

### macOS

- **豁免（待验）— macOS 13 真机**：构建目标声明最低 13.0，13 真机未验证。mvp-release 原文「构建最低13.0，13真机待验」。既有证据仅为本机 macOS 26.6.2/aarch64 实际运行（`docs/verification/packaged-macos/report.md`，vtool 确认主程序 minos 13.0），不替代 13 实机；原生 GUI 启动当时受 Mac 锁屏限制，边界见同报告。
- **豁免（当时待验）— 经 GUI 的备份恢复（安装包边界）**：mvp-release 原文「全部使用临时数据库，GUI恢复仍待验」——2026-10-07 时点的本机证据（新建→备份→预览→另库恢复→重启读取）全部走 CLI/临时库，未经安装包边界的 GUI 恢复。开发会话内 GUI 备份恢复与目录迁移回归另有通过记录（`docs/verification/studio-integration-20261007.md`），属开发环境证据，不构成逐 OS 安装验收。

### Windows

- **豁免（用户决策）— 安装/运行验收**：按用户 2026-10-07 决定，Windows 只要求产出 EXE，不做安装/运行验收；原构建目标表同时披露「干净机待验」。出处：`wardrobe-execution/results/continue-20261007/mvp-release-20261007.md`「用户最新调整：全部本地构建」。
- 历史 CI 时代（source 9b30572）的包内 CLI 证据明确 `gui_clean_install: pending`（`docs/verification/packaged-windows/report.md`），该边界不因本豁免改写。

### Linux

- **满足（限定范围）— 打包构建与包内运行时**：Ubuntu 22.04/24.04 分别实际生成 DEB + AppImage；包内独立 CLI 在移除开发加载环境后通过客户端 stdin 授权、无授权拒绝、HEIC 导入、退出重启读取与 MCP 等价查询；deb GUI 从包内私有目录加载 libheif/libde265，ldd 无 not found。出处：`docs/verification/packaged-ubuntu-22.04/report.md`、`docs/verification/packaged-ubuntu-24.04/report.md`（run37508783217，source 9b30572）。
- 边界：两份报告原文明确桌面 GUI 实际启动、系统安装/卸载/升级仍 pending，不因本条满足而关闭。

## 3. 2026-10-09 数据库版本不匹配事故

- **现象**：旧安装构建（schema v20 时代，commit e85653）启动时遇到生产库 `user_version=21`，按设计拒绝打开（「数据库由更新版本创建」）。版本门闩行为本身正确：`user_version > DATABASE_SCHEMA_VERSION` 即拒绝、只向前迁移（`crates/wardrobe-core/src/lib.rs:114-115`）。
- **性质**：与 2026-10-08 事故（v23 实验构建把 v21 生产库前向迁移，见 `wardrobe-execution/results/incident-schema-migration-20261008.md`）方向相反——本次是旧 schema 应用遇到更高版本库；数据库未被降级或改写，无数据损失。
- **当日处置与修复**：先把数据目录快照备份到 `/Users/links/.codex/external-workspace-volume/codex/wardrobe-execution/results/pre-mvp-fix-20261009/`（`data-dir-snapshot/` 含 wardrobe.sqlite 与 assets/，旧 APP 存档为同目录「旧安装-schema20-e85653-电子衣橱.app」）；随后换装 ticket35（issue #35 范围内、支持 v21 的构建），member.list、clothing.list 与 GUI 验证通过。
- **防复发**：issue [#51](https://github.com/MingStudentSE/wardrobe-agent/issues/51) 已关闭，代码级护栏已落地——debug 构建在未设置 `WARDROBE_DATA_DIR` 且未显式豁免（`WARDROBE_ALLOW_DEFAULT_DATA_DIR=1`）时以 `storage_error` 拒绝默认数据目录，release 构建行为不变（`crates/wardrobe-core/src/lib.rs` 的 `guard_debug_default_data_dir`/`default_database_path_checked`；测试 `crates/wardrobe-core/tests/data_directory_guard.rs`、`crates/wardrobe-cli/tests/data_dir_guard.rs`）。环境与流程护栏见 `docs/agents/build-storage.md` 第 8 条。

## 4. 发布前门槛逐项现状

mvp-release-20261007.md「发布前仍须满足」清单在 2026-10-09 的现状：

- **满足 — 真实供应商图像验收（#14 商品展示图 / #20 AI 搭配图 / #21 虚拟试穿）**：2026-10-08 已完成 ARK `doubao-seedream-5-0-flash-260915` 真实云调用验收，request_id、真实耗时、usage token 与结果图 SHA256 存证齐全，三票已关闭；mvp-release 基线中「全部为本地协议/fixture、#14 真实供应商生成质量未验收」对这三票已被取代。出处：`docs/verification/ticket-14/report-real-cloud.md`、`docs/verification/ticket-20/report-real-cloud.md`、`docs/verification/ticket-21/report-real-cloud.md`；外盘存证 `wardrobe-execution/results/continue-20261007/real-ai-acceptance/`；审计笔记 `wardrobe-execution/notes/spec-20261009/real-ai-acceptance-audit.md`。F01–F30 对照现状另见 `docs/verification/ticket-35/final-gate.md`（准备稿，不勾选发布门槛）。
- **缺失（开放）— 其余各项**：冻结并审查源码；三平台同源码产物及 SHA256；产品许可证授权与完整第三方许可清单（含 MSVC CRT）；逐平台干净安装、升级、卸载数据保留、备份恢复；真实可用 GUI 入口。#35 轨道已有一轮 DMG 安装/升级/卸载工程记录（`docs/verification/ticket-35/final-gate.md` F24 行，终验待 #34 合入后执行），本条按 #37 MVP 轨道口径保留开放。上述门槛未满足前不发布下载、不关闭 #37。GitHub Actions 账户阻断现状与恢复条件见 `docs/verification/integration-ci-account-block.md`。
