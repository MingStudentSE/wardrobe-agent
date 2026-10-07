# wardrobe-agent

本地优先的电子衣橱与 AI 穿搭助理。

## 项目状态

已集成完整 React/Tauri 前端、共享 Rust 核心与独立 CLI/MCP；新版衣橱工作室界面已在本机桌面端安装验证，完整 Beta 交付仍以规格验收为准。

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

Beta 全部交付 F01–F30，本轮仅发行 macOS；Windows/Linux 发行延后，保留跨平台架构，Android 后续。产品不提供收费或付费生成入口，模型调用使用用户自行配置的 API 额度。字段、操作流程、全部能力和验收标准见 [Beta 开发规格](docs/05-Beta开发规格.md)。

## 本机客户端授权

在 GUI「客户端授权」登记操作 scope、成员范围和到期时间。完整业务授权允许范围内连续调用，无需每次确认；连接令牌仅返回一次。令牌与供应商 API key 分开，不保存到普通配置、命令参数或日志。

外部 CLI 使用 `wardrobe-cli <operation> --client-stdin`，从标准输入读取 `{ "token": "连接令牌", "input": { ... } }`。无授权业务请求返回 `unauthorized`。MCP 独立启动 `wardrobe-cli mcp`，通过 stdin 的 `initialize.params.authorization_token` 连接授权后复用，或者在 `tools/call.params.authorization_token` 提供令牌；每次操作仍检查 scope、成员、撤销及到期。

本机人类维护显式使用 `--trusted-local`，等同于拥有数据目录的人工维护权限；不要把该入口提供给外部客户端。该授权层不是同一 OS 用户之间的文件沙箱。设计与后续工具接入见 [ADR 0007](docs/adr/0007-local-client-authorization.md)。

## MVP 收尾记录（历史范围）

以下保留本地已有 MVP 收尾记录；当前完整 Beta 范围及发行平台以本文上方和 Beta 规格为准。

研发试用版：衣橱录入、单品与图片管理、手工搭配、穿着、洗护、统计、推荐反馈、偏好学习和备份恢复已实现。当前优先完成 AI 商品展示图及三平台 MVP 安装包；其余未完成 Beta 功能暂缓，保留 GitHub 票据。详见 [MVP 现状与收尾范围](docs/06-MVP现状与收尾范围.md) 和 [MVP 发行任务 #37](https://github.com/MingStudentSE/wardrobe-agent/issues/37)。

当前源码与测试通过不等于正式安装包已发行。模型协议测试不证明实际生成质量；三平台安装、升级、签名与数据保留需要分别验收。
