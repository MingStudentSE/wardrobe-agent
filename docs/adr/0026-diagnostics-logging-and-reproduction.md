# ADR0026 · 全链路日志与诊断

状态：已实现，#41（票 42–50），LOG 独立文件。

全链路日志独立于 SQLite，这是五个决策里最根本的一个：结构化 JSONL 事件按天追加写入数据目录 `logs/` 子目录（`wardrobe-YYYY-MM-DD[.序号].jsonl`），不进数据库。数据库损坏、恢复失败或模式迁移中断时，日志仍可回查——「正是存储出问题的场景」必须有可读的痕迹；打开失败也写事件（`open` 事件带错误码与原因）。日志按构造与业务同生共死但互不拖累：写入尽力而为，失败静默降级并计数（`dropped_events` 经 `diagnostics.summary` 暴露），绝不阻塞或改变业务操作结果。日志汇每进程按数据目录幂等注册；GUI 的 application 命令每次 `Application::open` 都会初始化 logger，旁路命令（如桌面端天气城市搜索，不经 Application）经进程内幂等的 `log_bypass_event` 写入同一目录，事件在此之前被调用也可用。

链路标识贯穿：每次 Application API 调用分配 UUIDv7 `trace_id`，同一调用的日志事件与错误输出共享此值，是用户反馈与 `diagnostics.logs.query` 的精确锚点；每次进程启动生成一次 `run_id`（GUI 每命令重开 Application 不产生新 run_id），跨天、跨文件的同一运行以 run_id 关联。同步链路的 trace_id 走线程局部上下文（`with_trace`）；异步持久任务是跨进程边界——worker 可能运行在另一进程，trace 只经任务文档传递（创建调用的 trace_id 写入 jobs 文档，worker 事件从任务文档读取），不依赖进程状态或调用栈以外的共享内存。任务文档因此是跨进程链路的唯一通道；供应商请求、job 入队与状态迁移事件都带它。任务事件在事务 commit 成功后才写入日志：commit 失败不残留未发生的事件，事件内容在保存时定格、字段逐字不变。

脱敏按构造：日志写入接口只接受安全标量（操作名、标识、耗时、错误码、原因、输入字段名），永不接受原始输入/输出 JSON 值——令牌、供应商密钥及其 hash、恢复材料、空间密钥与图片内容因此在构造上不可能落盘；文件路径与单品名允许。`ApiError::reason` 只写日志、不进用户可见输出（serde 跳过序列化）。诊断事件的自由字段（如 ui_event 的 label）用封闭白名单与长度上限约束，不接受自由文本；旁路事件只记结果类别与量纲（如天气城市搜索记 `query_length`，不记城市名全文）。

导出是单文件 JSONL（`diagnostics.export`）：首行 meta（kind、应用版本、操作系统、数据库模式版本、导出时间、覆盖时间范围、run_id 列表、按级别计数、最近错误摘要），其后为按时间排序的日志事件。范围 `recent` 默认最近 24 小时且跨重启（按事件 ts，不按进程），`session` 限本次启动，`all` 保留期内全部，或显式 from–to 起止时间窗（含端点，与 scope 互斥）；`kind=reproduction` 标记复现记录并附记录时刻的基础检查（`attach_checks`）。产物只来自日志文件与摘要投影，天然不含数据库、图片与凭据——导出与普通日志同一脱敏构造，可直接交给维护者。

基础检查只读且同源：`diagnostics.checks` 固定十项（数据目录可写、数据库模式版本、WAL、日志目录试写、数据卷剩余空间、本机安装代际、加密钱包状态、模型凭据是否已配置、卡住任务、待发布位置标记残留），各报 pass/warn/fail 加通俗中文消息、建议去向设置页标识与技术明细；只诊断不自动修复、不改配置或数据，唯一临时产物是试写探测文件且返回前删除。CLI `doctor` 与 GUI 诊断页执行完全相同的操作（summary + checks 同源）；数据库打不开时降级为文件系统级检查与 `database_schema=fail`——「打不开」正是最需要体检的场景。

默认参数：单文件 10 MiB、总占用 256 MiB、保留 14 天（日志轮转与清理）；`recent` 导出窗口 24 小时；最近错误窗口 20 条（摘要与导出同口径）；复现记录最长 600 秒（10 分钟），到点由前端计时自动停止，录制状态不持久化。日志级别在运行时仅能经环境变量 `WARDROBE_LOG_LEVEL=debug|trace` 提升（默认 info，error 级事件不受阈值约束），没有运行中的设置项——改级别需重启进程，诊断能力不因误操作静默增减。

迁移实际只建 `logs/` 目录与文件命名约定，无数据库模式变更。日志是本机数据，不进备份白名单、不随加密文件包携带；诊断导出是用户显式动作，由用户决定去向。成员与客户端 scope 由共同 Application 入口执行：diagnostics 操作仅本机可信入口（authorization::privileged 一律拒绝外部客户端），外部 AI 永远无法读取日志或体检结果。

源码追溯：[diagnostics.rs](../../crates/wardrobe-core/src/diagnostics.rs)（Logger、`log_provider_request`、`diagnostics_execute` 全部分发、doctor_report）、[jobs.rs](../../crates/wardrobe-core/src/jobs.rs)（`PendingJobEvent`、任务文档 trace_id）、[lib.rs](../../crates/wardrobe-core/src/lib.rs)（`logged` 统一计时与 trace 注入）；入口：[main.rs（桌面）](../../apps/desktop/src-tauri/src/main.rs)（旁路天气搜索事件）、[main.rs（CLI）](../../crates/wardrobe-cli/src/main.rs)（logs/doctor）；界面：[DiagnosticsPanel.tsx](../../apps/desktop/src/components/DiagnosticsPanel.tsx)（按 run_id 分组的最近错误）。
