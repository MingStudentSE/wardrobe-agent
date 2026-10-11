# ADR 0040：outbox 停写与同步范围单一声明

状态：已采用（2026-10-11，集成 code-review 补记；停写本身由 #71 实施并合并）。

## 背景

ADR 0001 早期写明「领域事件当前仅写入本地 outbox；它不是已实现同步的声明」，并把「单品、audit 与 outbox 同一短事务提交」作为初始化事务的描述。随着 #27/#32/#33 落地真实同步后，这两句只剩历史意义：真实的发布链是 `SYNCED_TABLES` 各表的 `sync_{table}_{INSERT,UPDATE}` 触发器写入 `sync_journal`，按目标登记于 `sync_publications`（提交标记见 sync/index.rs），从不消费 outbox。#71 复核确认 outbox 全库零读路径，随即清零全部 22 个写点；但决策依据（为什么停写、表为什么不删、同步范围以什么为准）此前只散落在代码注释与工单里，与 ADR 0001 的原句冲突，审查要求补一份决策记录。

## 决策

1. **outbox 停写**：领域事件不再写入 outbox。理由：零读者——全库没有任何路径读取 outbox 行，它从未成为已实现的同步通道；真实发布链是 sync_journal 触发器 + `sync_publications` 目标登记 + 提交标记，事件凭据经 `commit_job_events` 在事务出口发射（jobs.rs）。ADR 0001 的原句保留不改写，由句末注记指向本 ADR。
2. **表结构保留一个发布周期**：按「schema 迁移只向前、不可逆」的项目约定（AGENTS.md Data directory isolation），`outbox` 的 `CREATE TABLE IF NOT EXISTS` 与既有数据行原样保留：旧版本构建仍可打开同一数据目录，历史行继续进便携备份（backup.rs TABLES）；同版本内不删表，删除留待停写后的一个发布周期由后续迁移显式决定。
3. **同步范围单一事实来源**：哪些持久事实进同步、哪些明确排除，以 `sync::SYNCED_TABLES` / `sync::SYNC_EXCLUDED_TABLES`（sync/mod.rs）为唯一声明，由 `tests/sync_scope.rs` 对 sqlite_master 焊住；扩大或缩小同步范围必须先改这两份清单，而不是改 ADR 或注释。便携备份范围是另一份清单（backup.rs TABLES），与同步范围刻意不同，不互相推导。

## 后果

- 「同步不回声」等既有表述中「写入 outbox」的部分失效，GLOSSARY 已同步修正为审计与本机回执记录。
- outbox 停写后 `audit` 仍是本机操作台账（只进便携备份），事件发布语义完全由 sync_journal 链路承担；不存在「双写不一致」窗口。
- 后续若决定物理删表，须新增 ADR 并保持向前兼容（旧库打开不受影响）。

## 测试证据

- `crates/wardrobe-core/tests/outbox.rs`：业务写路径（单品、搭配、穿着、洗护、推荐、任务等）执行后 outbox 计数为零行。
- `crates/wardrobe-core/tests/sync_scope.rs`：同步范围以 `SYNCED_TABLES`/`SYNC_EXCLUDED_TABLES` 声明为准对 sqlite_master 逐表焊接。
- 同步行为契约由 `tests/sync_storage*.rs`、`tests/sync_ops_lifecycle.rs` 等既有文件守护，#71 停写不改变其结果。
