# ADR 0001：共享本地 Application API

状态：已采用（2026-10-07），父规格 #1、垂直切片 #2。

以 Rust `wardrobe-core::Application` 为公开命令/查询边界，返回版本化 JSON 与 ApiError；React/Tauri、CLI、stdio MCP 均调用同一实现。GUI 未运行时 CLI/MCP 直接打开同一数据库。默认数据目录在 macOS 为 `~/Library/Application Support/wardrobe-agent`、Windows 为 `%APPDATA%/wardrobe-agent`、Linux 为 `$XDG_DATA_HOME/wardrobe-agent`（缺省 `~/.local/share`）；`WARDROBE_DATA_DIR` 为三个入口一致的显式覆盖。CLI 另支持 `--db`。

SQLite 使用 bundled SQLite、WAL、外键与 5 秒 busy timeout。初始化的 IMMEDIATE 事务串行化跨进程迁移；单品、audit 与 outbox 同一短事务提交（2026-10-11 起 outbox 停写，表结构保留一个发布周期，见 ADR-0040）。后续迁移须检查 user_version 并保持跨进程串行化。领域事件当前仅写入本地 outbox；它不是已实现同步的声明（2026-10-11 起 outbox 停写，真实发布链见 ADR-0040）。

操作发现只返回已实现能力。无图片录入字段、真实图片、模型、任务和同步能力随后按依赖票实现；这些能力不能由占位函数替代。后续新增命令在同一边界完成跨入口验收（2026-10-11 起公开操作以注册表为单一数据源，分发/字段白名单/effect/授权元数据从它派生，见 ADR-0041）。

单品以 UUIDv7 标识，UTC 时间戳，active 生命周期。成员参数必填，未知成员返回 not_found，不允许退回默认成员。默认空间初始化幂等，默认成员稳定 ID 为 default，未来独立成员使用 UUIDv7。默认 ID 属初始化种子，单品不使用自然键。

2026-10-07 首轮审查补充：默认数据库路径计算提取为核心公开函数，适配器不复制平台判断。DB storage version2 在同一 IMMEDIATE 事务内迁移早期 `price_minor` 与 `jumpsuit` 为 canonical 字段/类别并补 ISO 币种指数，同时更新 audit/outbox 的早期文档；事实、身份和原有时间戳保持。JSON Application API 合约版本仍为1。旧版含版本保护的程序拒绝打开较新数据库；错误迁移回滚，不将损坏数据当空库存。

2026-10-07 独立成员 #3：DB3 在已有 IMMEDIATE 迁移事务中创建持久 workspace，默认空间和默认成员均使用 UUIDv7；初始化名字为“我”。迁移早期 local/default 合成身份时保留单品 ID、audit/outbox 行和实体 ID、原始时间戳及自定义成员名，仅替换所属成员身份。旧 CLI 的 default 参数作为兼容别名解析为持久 UUID，不出现在新输出中。成员迁移以实际 workspace 数据判断幂等，不能单独依赖全局 user_version。member.create/update/list/select 贯通共用 Application，选择持久化；成员更新仅改名称，不引入邀请、角色或协作权限。
