# #10 洗护管理验证

F25 垂直切片：Application、SQLite DB13、GUI、CLI/MCP 贯通；care.configure / mark_dirty / record / correct / revoke / status / get / list / history / dirty_list / reminders / availability 可通过既有客户端 operation/member scopes 调用。

首轮 public Application + 临时 SQLite 的 unsupported_operation 红灯，随后阈值/手动待洗/lifecycle 绿灯；明确意图/配置revision遗漏红灯后校验修复，时间补记、更正、撤销/maintenance默认不重置通过。后续已执行时区/DST、跨成员、重启、回收站历史、实际缺表重建测试。没有 mock 内部事实读写或模型服务。提示、计次与顺序策略见 ADR0017。

GUI 是 Chrome → 真实开发bridge → CLI/Application/SQLite，非预置库存响应。两项洗护操作与响应丢失幂等重试测试已通过；截图检查320px说明和关键按钮完整可见，七宽度无横向溢出。原生Tauri窗口未操作；窄屏不是Android交付。

未启动远端CI、未推送或关闭Issue；全Beta35项门槛仍由父任务验收。最终 integration 基线295e3fd（已含 #13 完整识别业务 DB12 与 #26 encryption DB10），当前最大 DB13，三模块真实表均初始化；缺失 care 表时按实际表重建，不降版本。

`cargo test --workspace --locked` 终态 exit0：129 passed / 8 ignored，包含 care9 项和 CLI20项（其中新增care实际进程/MCP/stdin授权测试1项）。覆盖默认未知阈值与CAS、manual dirty时间线、wash/dry_clean与maintenance、补记/更正/撤销/重启幂等、UTC/IANA/DST/未来日期、跨成员隔离、回收站快照、缺表修复、同件跨两模板计次、配置UUIDv7/creation稳定、更正未显式替换备注时保留事实。

clippy --workspace --all-targets --locked -D warnings、fmt check、desktop build 实际 exit0；终态日志与首轮红绿在 [logs](logs/)。8项 ignored 是既有真实OS凭据/模型门槛，本轮未运行，不当作原生安全存储/模型成功。本模块没有外部识别调用，也未读取用户照片或key。

GUI完整首轮36 passed /4 skipped；第二轮与Rust重链CLI重叠时洗护保存出现dev_bridge_unavailable，失败日志保留为 gui-concurrent-relink-failure.log。CLI被替换是推测原因，不当成已验证原因；后续在所有Rust/CLI构建终态后序贯GUI整套：37 passed / 4 skipped，exit0，包含3项care真实后端测试。4项skip为既有需要实际模型/OS安全存储的门槛，本轮不执行且不计通过。

独立预审发现只读历史迟到响应风险，已补success/error epoch guard，并实际延迟真实HTTP读取、切成员证明新成员不会显示旧历史。CarePanel在root本已按member key卸载，这项守卫同时保护组件复用。Hallmark既有Atelier tokens与组件语义保持，新增局部CSS不重写全局；七宽度断言与320/1280截图提供适配证据。

独立merger审查补拒绝冲突更正意图与一致读取快照，并增加撤销历史资产保留/并发配置快照测试；本轮132 Rust passed/8 ignored、37 GUI passed/4 skipped。详见 [独立审查报告](merger/report.md)。功能结案由父任务决定，未替代原生全Beta门槛。
