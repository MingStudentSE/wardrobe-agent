# ADR 0039：全链路日志与诊断系统（独立于 SQLite、trace 贯穿、按构造脱敏）

状态：已实施；分支 `codex/ticket-41-diagnostics`（基于集成 tip bdbd06e），红/绿证据在 `wardrobe-execution/results/continue-20261007/ticket41/`。

## 背景

#41 要求应用出问题时有可回查的记录：错误的用户可见消息保持宽泛（如 `storage_error`「本地数据库操作失败」），但底层真实原因（SQLite 具体错误、供应商失败细节、任务重试/租约接管过程）在错误转换时被丢弃，磁盘无迹可寻；普通用户也没有不依赖命令行的导出与体检入口。

## 决策

1. **日志独立于 SQLite**：日志写数据目录 `logs/` 子目录的 JSONL 追加文件（经既有数据目录解析，跟随 `WARDROBE_DATA_DIR` 与 profile），按天分文件；单文件 10 MiB、总量 256 MiB、保留 14 天为默认上限（`Application::open_with` 可注入极小值驱动测试），启动与写入时惰性清理。数据库损坏、恢复失败、数据位置切换失败时日志仍可查询；写入尽力而为，失败静默计数（`diagnostics.summary.logs.dropped`），绝不阻塞业务路径。
2. **trace 贯穿**：每次 `Application::execute` 分配 UUIDv7 trace_id（线程上下文贯穿同步链路）；任务文档新增可选 `trace_id` 字段（任务 schema 加法变更，outputSchema 同步声明）；`job_claim` 后链路延续为任务原始 trace，worker 执行与供应商请求复用同一编号；任务 enqueue/claim（含租约接管）/finish（含重试排队）/cancel/retry 均有生命周期事件。每次进程启动经 `begin_run()` 记为运行会话（run_id），事件统一携带。
3. **原因保留与错误输出**：`ApiError` 新增 `trace_id`（序列化输出，加法变更）与非序列化的内部 `reason`；`From<rusqlite::Error>` 不再丢弃真实错误（写入 reason，由 execute 统一落日志），用户可见 `{code,message}` 中文消息语义不变。级别映射：输入/授权/状态类拒绝为 warn，存储与系统故障为 error；默认 info 级，`WARDROBE_LOG_LEVEL=debug|trace` 提升。
4. **脱敏按构造**：日志事件只携带显式挑选的字段（操作名、trace、结果码、耗时、输入摘要），detail 再经 denylist 兜底清除（api_key/token/secret/password/recovery/space_key 等）；绝不记录供应商密钥、连接令牌及其 hash、恢复材料、空间密钥与图片内容；文件路径与单品名允许。导出产物不含数据库、图片原图与任何凭据。
5. **诊断面仅本机可信入口**：`diagnostics.logs.query / summary / checks / export / ui_event / reproduction.start / reproduction.stop` 进共享 operations 注册源，scope 元数据由授权规则同源派生（registry 契约测试钉住）；全部列入 `privileged()`，外部客户端授权请求包含它们一律 `scope_denied`/`invalid_input`，capabilities 对外部客户端不暴露。GUI 基础检查与 CLI `doctor` 共用同一套 `run_checks`/`build_summary`（检查只读，不自动修复）。
6. **导出单文件 JSONL**：`diagnostics.export` 首行 meta（应用版本、操作系统、数据库模式版本、导出时间、时间范围、run_id 列表、按级别计数、最近错误摘要、丢弃计数），其后按时间升序的事件行；范围 `session`（本次运行）/`recent`（默认最近 24 小时，跨重启）/`all`/显式时间窗；`kind=reproduction` 的复现导出附记录窗口与当时基础检查结果，产物可直接外发。
7. **备份边界**：便携事实备份、加密文件包与数据位置切换发布集均不含 `logs/` 与 `exports/`；日志不占用 stdout/stderr，MCP stdio 会话不受污染（CLI 契约测试钉住）。

## 后果

- 诊断摘要/检查在数据库损坏时降级而非中止（doctor 以 conn=None 路径输出 fail 检查项）。
- 诊断页第 9 张卡与全局错误反馈条「导出日志」按钮提供一键导出；桌面端经 Tauri dialog 保存并可打开所在文件夹，dev/浏览器回退写数据目录 `exports/`。
- 既有严格 outputSchema 断言同步了任务 schema 的 `trace_id` 加法变更；错误对象新增 `trace_id` 属加法变更，CLI JSON 契约不变。

## 测试证据

- `crates/wardrobe-core/tests/diagnostics.rs`（11 项）：trace/run 落盘、失败原因只写日志且用户消息不变、密钥不落日志与导出、过滤与游标、导出范围与跨 run 窗口、轮转与保留封顶、summary 与任务实际状态一致、健康/故障注入检查、复现窗口与自动停止、外部客户端拒绝、创建→任务→worker→供应商链路贯穿。
- `crates/wardrobe-cli/tests/diagnostics_entry.rs`（4 项）：logs 过滤契约、doctor 与 GUI 检查同源、数据库损坏时 doctor 降级、MCP stdio 不被日志污染。
- `apps/desktop/tests/diagnostics.spec.ts`（4 项）：诊断页与导出落盘、错误反馈条一键导出（meta 首行校验）、复现记录窗口导出、doctor 与页面检查同源。
