# wardrobe-agent

本地优先的电子衣橱与 AI 穿搭助理。

## 项目状态

当前工作分支已集成完整 React/Tauri 前端、共享 Rust 核心以及独立 CLI/MCP。新视觉系统采用 ui-ux-pro-max 与用户确认的私人衣橱工作室方向；尚未完成的 Beta 规格能力不视为已交付。

设计方向为桌面优先的 React + Tauri、共享 Rust 核心与每设备 SQLite，提供 GUI、CLI 与 MCP 入口。详细范围与设计决策以 `docs/` 中的文档为准。

## 文档

- [阅读入口与研究结论](docs/00-阅读入口与结论.md)
- [需求清单](docs/01-需求清单.md)
- [Agent 功能与项目融合](docs/02-Agent功能与项目融合.md)
- [架构草案](docs/03-最终架构.md)

## UI 设计

使用 `ui-ux-pro-max` 设计技能。Beta 只交付桌面端，同时为后续 Android 保留共享视觉语言、窄屏布局与触摸操作基础。参见 [UI 设计与跨端适配](docs/04-UI设计与跨端适配.md)。

## 本地参考资料

`references/` 保存用于研究的第三方仓库，不纳入本项目版本管理。来源、固定提交与许可证限制见研究文档。

## Beta 交付规格（2026-10-07）

Beta 全部交付 F01–F30，同时支持 Windows/macOS/Linux；Android 后续。产品不提供收费或付费生成入口，模型调用使用用户自行配置的 API 额度。字段、操作流程、全部能力和验收标准见 [Beta 开发规格](docs/05-Beta开发规格.md)。

## 本地界面开发

`apps/desktop` 为实际前端，`design/studio` 为独立设计参考。真实前端通过 `api.ts` 调用共享 Application API；开发网页桥接本机 CLI，生产桌面通过 Tauri invoke 调用。

运行前先构建 CLI，并配置本机媒体运行库；随后在 `apps/desktop` 执行 `npm run dev`。CLI 使用 `wardrobe-cli capabilities`，独立 MCP 使用 `wardrobe-cli mcp`，外部业务调用须在设置中登记授权。
