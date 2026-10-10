# ADR 0027：本地文件夹双设备同步的删除语义、包白名单与资产发现预算

状态：已实施；ticket-27 同步分支 `codex/ticket-27`，红/绿证据在 `wardrobe-execution/results/continue-20261007/ticket27/`。

## 背景

#27 交付真实双设备增量同步的最后缺口：删除 tombstone 与受保护删除的跨设备往返、跨成员隔离、重复同步幂等、包内容白名单、中断上传不误应用，以及资产发现扫描的预算与公平性。深度并发冲突（乱序、时钟偏差、删改冲突副本）仍归 #28；本票只承诺按发布顺序（journal id / UUIDv7）的顺序同步正确，`sync_fields` 变体寄存器与 `sync_immutable` 分支保持为 #28 的接口。

## 删除与受保护删除

- 单品删除是软删除：`clothing.delete` 写 `status=deleted`、`deleted_at`、`status_before_delete`，经 clothing UPDATE 触发器进入同步日志；对端按序应用同样进入回收站，`clothing.restore` 修订可再次激活。字段冲突裁决中 `status=deleted` 优先于并发编辑值（迟到编辑不复活删除，并发副本保留给 `sync.conflicts`/lifecycle）。
- 分类删除受保护：`other` 不可删除；删除自定义分类在同一事务内把引用单品迁入 `other`（逐条审计与 outbox），再写 `wardrobe_categories` 墓碑行（`deleted=1`）。跨设备按发布顺序到达：先衣物迁移修订、后分类墓碑；全新设备经引导只见到当前事实（衣物在 `other`、墓碑分类不在列表）。
- `materialize` 对 arriving clothing 的类别校验保持：仅默认分类或本成员未删除的自定义分类可作为存活事实的目标；缺失依赖进入 pending 公平重放，非法引用进入公开 quarantine。

## 包内容白名单

同步包只允许 `{space}/changes|blobs` 与 `{space}/epochs/{n}/changes|blobs|index/...` 下的加密 `.commit/.object/.page/.index` 对象及传输本地 `.lock` 边车；不创建 `media/`、不携带数据库文件。测试在外部扫描全部远端对象字节：原图字节、源文件名、空间口令、模型凭据（OS 钥匙串保存的 bearer key）一律不得出现；`assets.get.original` 在对端恒为 `null`，展示图与缩略图由对端从密文展示字节本机重建。回收站/占位设备仍可编辑与记录穿着（占位 stub `display.pending=true`，blob 到达后重试替换并生成本机缩略图）。

## 中断与损坏

上传按 object→marker 两步 `CreateOnly` 发布；marker 不可见则对端发现不到该操作（不应用半成品）。对象被截断或损坏时，marker 摘要校验整体失败（`storage_corrupt`），该对象的部分字节永不进入事实；修复字节后同一日志无需重新发布即收敛。本地文件夹 `put` 使用 staging+rename+fsync（文件与目录），进程中断不会留下半写对象。

## 资产发现预算与公平性

- 新表 `sync_asset_commits(key TEXT PRIMARY KEY)`：已接受并排队的 blob commit 键永久登记，重复扫描跳过解密（键为 `asset:{id}:{blob_hash}.commit`，内容寻址；重新采用产生新 hash 键自然重扫）。
- 新表 `sync_blob_scan(space_id,folder,last_key)`：每个目录每次同步批最多列举 16 页（64 键/页），预算耗尽时保存续扫位置，列举到末尾时清除（回卷），迟到提交与慢发布者的资产最终被发现，单个大目录不能垄断每批工作。
- 未登记 commit 损坏仍整体失败（无法验证的新对象必须可见地失败）；已登记 commit 事后损坏不阻塞后续同步（对端已具备该资产字节）。便携恢复会清空两张表与整个同步传输状态（`reset_after_restore`），恢复后的重发现从零开始。
- 两表由 `sync::migrate` 无版本门控 `CREATE TABLE IF NOT EXISTS` 创建；`sync::migrate` 在每次打开时无条件运行（先于 `learning::migrate`），因此 schema 23（#25 调度）落地时无需同步侧改动；跨版本升级后的同步连续性由重开数据库的公开回归覆盖。

## 测试与证据

- 新公开回归 `crates/wardrobe-core/tests/sync_dual_device.rs`（5 个真实双安装 ignored 测试）：删除/恢复/受保护分类删除+第三设备引导、双成员隔离+事件历史分支一致、重复同步幂等+包白名单+重开数据库后再同步、截断对象整体失败与 marker 缺失不可见、资产发现跨成员公平收敛+历史提交损坏不阻塞。
- 真实 CLI 进程证据：两个真实数据目录 + 共享本地文件夹 + 每步独立 `wardrobe-cli` 进程，配对→发布→截断→整体失败→修复收敛→双端幂等→白名单扫描，全部退出码与 JSON 输出存档于 `wardrobe-execution/results/continue-20261007/ticket27/cli-evidence/`。
- `sync_legacy_receipt` 的 schema-20 夹具修正：真实 schema-20 备份不可能包含 schema-21 的 `wardrobe_categories` 表，夹具导出副本删除该表后再标 `user_version=20`；生产校验不变。

## 边界（留给后续 ticket）

乱序/重复事件计数、删改冲突副本、时钟偏差、跨安装收敛策略与因果深检属 #28；S3/WebDAV 后端复用同一 `SyncStorage` 契约（#29/#30）；暂停/重试/更换目标/密钥生命周期与按水位回收 tombstone 属 #31；GUI 端到端点击验收与原生通知集成归 #34/#35 的全应用验收。
