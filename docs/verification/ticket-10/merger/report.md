# #10 独立合并审查

基线 integration `295e3fd`，候选 `5f660209`，独立 `codex/merge-10` / `/tmp/wardrobe-merge-10`，no-ff 合并无冲突。读取 Issue10、F25、ADR0017与实现报告后检查 Application、有效wear/care时间线、API/UI、资产保留及CLI/MCP。

## 审查修复

1. **更正意图不静默忽略字段**：原 `care.correct` 同时提供 action+dirty 时会按原事件类型静默忽略其中一项。公开 Application 测试实际先返回成功（red日志），修复为写入前明确 invalid_input，清洗记录及手动标记两类均验证历史没有新增修订。GUI正常更正仍只发送一种意图。
2. **读取快照一致性**：原 status/availability/dirty_list/reminders 由多次 SELECT 组成，autocommit不能保证跨进程写入时读取同一版本。只在连接autocommit时建立DEFERRED读取事务，递归读取与已有IMMEDIATE写事务复用当前事务，不嵌套。真实两SQLite连接的测试运行400次原子双配置更新、200次公开availability读取，所有返回同一配置组，之后公开写入仍成功。没有声称旧实现的竞态已在本机复现；这项来自事务边界审查。

## 领域与持久化

- UTC时间线排序、同刻先wear后care、date精度不确定提示一致；wash/dry_clean重置此前穿着和手动dirty，maintenance不重置，后来手动标记继续生效。
- 默认阈值null、正整数/配置CAS、完整操作收据、UUIDv7/稳定created_at与配置身份；更正未显式替换备注则保留。阈值到期不阻止如实记录实际wear，生命周期独立。
- 回收站拒绝新洗护但旧快照可更正/撤销；资产引用保留所有历史/撤销，独立新增真实PNG导入→采用→护理→撤销→detach→删除单品→重启，asset仍adopted/workspace且历史与sync manifest保持展示图。
- DB13按真实表初始化/补缺，不降低最高版本；独立重启确认recognition_suggestions、encryption_state和四个care表共同存在。原缺care表重建测试通过。
- 可用性与提醒是明确query（available/reasons、notification_scheduled=false），供后续推荐/调度消费；此模块不声称已发自动通知。
- operation/member scopes共享；现有CLI→授权stdin MCP护理记录/提醒流程实际通过。UI历史epoch守卫保护组件复用；main原有member key卸载本就隔离成员，没有将其描述为原先已观察到的泄漏。

## 实际独立检查

`cargo test --workspace --locked` **132 passed / 0 failed / 8 ignored**，care12项（新增3项）。`cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、CLI build、desktop build通过。

独占09缓存使用debug0、j2、incremental0及 `/tmp/wardrobe-media-native/install` nativeprefix。首次workspace链接因ENOSPC停止；只删除24个此前终态旧项目可执行文件（约2.88GB），保留依赖和release实际模型产物后重跑通过。这不是源码失败。所有Rust/CLI构建终态后才启动独立Chrome GUI，终态计数补于下文。

## 验收范围

建议父任务在合入并确认本报告后按 #10/F25 功能验收结案，届时才释放 #15 frontier。原生Tauri三平台GUI与整个Beta打包/模型/加密平台门槛不因本报告宣称完成。此前实现者dev_bridge_unavailable日志的原因仍未验证，不将“重链导致”改写成事实。

最终独立Chrome GUI整套 **37 passed / 0 failed / 4 skipped**（1.2m），在Rust/CLI全部终态后序贯执行，无dev_bridge_unavailable。本轮3个care界面测试覆盖阈值、手动待洗、干洗补记、更正/撤销、reload、七宽度、丢失响应幂等重试与成员锁、迟到真实历史读成员隔离。实际320px截图人工核对可读、关键按钮完整、Atelier与既有视觉一致；原生Tauri窗口未操作。4项skip为既有OS安全存储/真实模型服务门槛，不算通过。完整输出见 [gui.log](gui.log)。

临时target借用链接已移除；没有push、关闭issue或干预任何CI。报告的132/37为本轮独立真实终态计数，不复用候选分支结果冒充本轮执行。
