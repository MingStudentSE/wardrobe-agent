# RELEASE NOTES（草稿）：电子衣橱 macOS 完整 Beta 候选 0.1.0

日期：2026-10-08。分支 `codex/ticket-35-release`（起点 `codex/beta-integration-continue` tip `464a845`）。
状态：**Beta 候选，非正式发布**——最终发布门槛见文末「发布前待办」。

## 产物

| 产物 | 说明 | SHA256 |
| --- | --- | --- |
| `电子衣橱_0.1.0_aarch64.dmg` | macOS Apple Silicon（arm64），最低系统 macOS 13.0，含桌面应用与内置 wardrobe-cli sidecar | `aaf81895459c19a25a325f89702d1ef610723cfede842fc6367b5675b98c1fc6` |
| `wardrobe-cli-portable/` | 独立 CLI（无需源码/Rust/GUI；`lib/` 内含可替换 LGPL 动态解码库与许可文本） | `f2d7258cad72d57b63e27e63fb9122babff6b255bc799e58f772ee9b8c9fceee` |
| `wardrobe-cli-sidecar/wardrobe-cli-aarch64-apple-darwin` | Tauri externalBin sidecar（与 portable 同字节） | 同上 |

签名状态：ad-hoc 签名，**未公证**（无 Apple Developer 证书）；首次打开需右键→打开或在系统设置放行。DMG 由 `bundle_dmg.sh --skip-jenkins` 生成（无自定义图标布局，功能等价）。

## 本轮功能摘要（相对上一公开候选，集成 `codex/beta-integration-continue` 118 提交）

- **场景与旅行计划**（#23）：婚礼/面试/多日旅行搭配与去重打包清单；计划不计入实际穿着。
- **购买适配分析**（#24）：候选商品草稿、与真实库存的搭配潜力/重复购买分析；候选不入库存。
- **内置 Stylist 受控写入**（#22）：显式指令下的穿着/洗护/编辑等记录类工具调用。
- **双设备本地文件夹同步**（#27）与**同步冲突/删除/事件一致性**（#28）：字段合并、冲突副本、tombstone、缺图占位重试。
- **S3-compatible 后端**（#29）与 **WebDAV 后端**（#30）：统一 SyncStorage 契约、条件写能力探测、真实双设备往返。
- **同步运维与密钥生命周期**（#31）：暂停/重试、校验后更换目标、镜像备份、改密码/撤销设备/密钥轮换与水位回收。
- **定时推荐与系统通知**（#25）合入：成员计划、暂停/修改、休眠补执行与去重；macOS 原生通知/自启动（LaunchAgent）。
- **四工作区 UI 重设计**（#37/#38，此前已确认）为基础的完整界面。
- 桌面发行工程：媒体分发（libheif/libde265 framework + 独立 CLI + 许可资源）、DMG 打包、独立 CLI 安装形态。

完整功能边界以父规格 F01–F30 为准；未完成项见「已知限制」。

## 已知限制（如实）

1. **真实云 AI 验收未完成**：#14 商品图、#20 AI 搭配图、#21 虚拟试穿的实现已合入，但真实云图像调用成功证据待用户配置模型供应商密钥后验收；本地/离线路径（识别纠错、规则推荐、离线拼贴、本地抠图 U2NetP）已有真实证据。
2. **全应用 UI 终验进行中**：#34（集中设置/视觉一致/键盘/窄屏）由并行工作收尾，合入后需对本发行候选做门禁复核。
3. **未公证、未 Developer ID 签名**：ad-hoc 签名；macOS 13–15 真机未实测（构建目标 13.0，实测环境为 macOS 26 / Apple M4）。
4. **CI 账户阻断**：GitHub Actions 因账户付费/支出上限失败（run 37518939745 等，`docs/verification/integration-ci-account-block.md`）；本轮产物为本地构建并有完整日志，云端可复现构建待账单恢复。
5. **Windows/Linux 延后**：2026-10-07 用户决策；跨平台架构审查已做（`architecture-review.md`），但不代表 Windows/Linux 已构建或验收。
6. **性能基线硬件高于父规格基线**：实测 Apple M4/16GB（父规格 4 核/8GB），数字与口径见 `performance-baseline.md`。
7. **原生窗口截图受限**：验证会话屏幕锁定，GUI 启动证据以窗口记录+数据行为准，视觉截图经同一前端构建渲染（`install-upgrade-uninstall.md` 第 0 节）。
8. 产品开源许可证未选择（仓库无 LICENSE）；第三方许可清单见 `THIRD-PARTY-LICENSES.md`。

## 迁移 / 升级说明（用户）

- **升级**：直接用新版应用打开既有数据即可。数据库带版本号，应用自动逐级迁移（实测：schema 20 → 23 迁移后数据完好，`docs/verification/ticket-35/install-upgrade-uninstall.md` 第 2 节）。
- **降级不支持**：旧版本应用打开新版本创建的数据库会明确报错「数据库由更新版本创建，请升级应用后再打开」，不会损坏数据。
- **卸载**：把应用拖入废纸篓不会删除用户数据。`~/Library/Application Support/wardrobe-agent/`（数据库、原图资产、profiles.json）与 macOS 钥匙串中的加密空间密钥/模型供应商凭据会保留；重装同版本以上即恢复使用。彻底清除需手动删除上述数据目录与钥匙串条目（搜索 `wardrobe-agent`）。
- **备份建议**：升级前可使用应用内备份导出一致性备份（含/不含原图在导出时明示）。
- **外部 AI 入口**：安装后可在终端使用 `wardrobe-cli`（portable 目录含 README：PATH 安装、stdin 令牌授权、MCP 启动）；令牌一律走标准输入，不进命令行参数/日志。

## 发布前待办（不因本候选存在而免除）

1. #34 合入后对本候选做最终门禁复核（F23 与全 UI 回归）。
2. #14/#20/#21 用户云密钥下的真实生成验收。
3. 产品许可证决定并补 LICENSE；发布许可清单终版。
4. （可选/发布分发时）Apple Developer 签名与公证；恢复 CI 后冻结源码云端复现构建。
5. F01–F30 逐项对照终态确认（见 `final-gate.md` 现状）。
