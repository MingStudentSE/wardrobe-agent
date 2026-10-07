# ADR0025 · 可管理偏好与事件学习

状态：实现中，#18/F18，DB20。

显式偏好与推断画像分开。允许color/style/material/pattern/category/occasion特征，不接受尺寸、身体或衣物事实写入。显式偏好为用户声明，带说明、不可变修订与CAS；推断来自有效反馈和实际WearEvent当时快照，不读取当前衣物字段代替历史。用户编辑推断即创建明确覆盖，同特征明确值优先于推断，不修改原事件。

算法explicit-and-events-v1：每个事件内相同特征只计一次。like +2、dislike -2、skip/day_exclude/actual_wore/changed反应本身0；真实有效穿着事件+1。总体评分N-3作用于全部已知特征；舒适N-3仅作用material；风格N-3作用color/pattern/style。空评分0，无未知特征猜测。累计特征权重限制-5至5。来源完整保留有效反馈修订ID、有效穿着修订ID，事件更正/撤销后重算读取当前有效最新修订。相同源集合得相同有序信号与权重；新画像有自己的ID/时间。它是透明排序启发式，不能宣称统计预测准确度。

推荐在既有目标/场合等优先级之后用特征权重排序候选，共享规则组合与模型发布前再次检查当前固定/排除、反馈/可用性。画像不会绕过硬约束；既有推荐意图继续不可变，新显式推荐使用新操作标识。推荐context带算法版本和画像来源，用户可解释。

删除特征持久化suppression并立即移除当前影响；重置推断写入UTC记录时间边界，普通重算只读取边界之后记录的新有效事件。重置全部还追加显式偏好删除修订。事实事件永不删除。用户明确include_prior=true完整重建才清除suppression/边界并重新纳入此前历史，UI明确解释与确认，不由后台自动发生。管理历史保留修改审计，不让旧偏好重新生效。

迁移实际创建learning_memories、learning_profiles、learning_state、learning_history、learning_receipts，无条件修复缺表，数据库max20。成员与客户端scope由共同Application入口执行；GUI/CLI/MCP同一descriptor。业务同步/备份白名单需包含管理边界与历史及来源，禁止携带providers/jobs/credentials；协同后续模块统一serializer，不另写catchall导出。

### 持久重算任务与本机来源守卫

`learning.recompute` 只在短事务中保存不可变请求及用户记忆边界，立即返回 `job_id`。GUI、CLI 与 MCP 使用同一个 `learning.worker.run`；GUI 另提供查询、继续及取消。worker 用 DEFERRED 快照读取事实，在写锁外计算、序列化，最后短 IMMEDIATE 事务同时验证租约、取消、记忆边界和来源版本，并原子发布画像、终态回执、审计与 outbox。排队或运行期间的重置、删除、事实修订不会被迟到结果覆盖。客户端执行或控制此类任务还须具有原 `learning.recompute` 操作及成员授权。

五张学习业务表可移植；额外 `learning_source_versions` 是本机派生元数据，反馈与穿着事实的 SQLite 事务触发器维护 O(1) 来源版本，独立安装 generation 防止两机计数碰巧相等。该表、generation、租约、jobs 均不导出。备份仅移除画像顶层两个本机守卫字段，保留嵌套不可变证据；外来画像和来源变化后的旧画像可查看，但标记过期并不参与排序，须显式重算后启用。导入仍保留明确偏好、重置/删除边界与管理历史。

DB19 已验收的 23 表备份继续受原严格白名单验证，学习默认空；DB20 必须完整包含新增五张表。学习领域校验验证算法、显式权重/维度、修订链、状态边界、成员、事实来源特征与可复算权重，以及收据请求和结果引用。加密文件包也采用明确的反馈、推荐意图和学习投影，只有历史已授权展示资产可携带；这是配对安装的文件包读取边界，不承担事实合并、调度或远程同步。

### #19 单品锚点配对与配对学习补充（2026-10，已实现范围）

在 explicit-and-events-v1 之外新增独立配对算法版本 `effective-pairs-v1`：画像文档内嵌 `pair_projection`（schema_version 1 + 算法版本 + entries + 共享来源列表），entries 为排序去重的无序单品ID对及权重（领域上是无序共现，存储键排序仅为规范化），零权重省略，权重钳制 -5..5。投影由纯确定性函数从与画像重算相同的有效反馈与有效穿着事件计算，重算与可移植档案校验共用同一函数，不能分叉。配对权重：每个穿着事件内的去重成员两两 +1；反馈按 like +2、dislike -2 加 overall N-3（其余反应 0）。锚点反馈（controls.pairing_anchor_id）只教锚点—成员边，不教其余两两边；普通反馈与穿着教全部无序不同对。

`pairing.generate` 是封闭公共输入的薄封装：锚点转换为 `pairing_anchor_id` 并追加进 `fixed_ids`，随即走既有共享 `recommendation.generate`——原子请求/重放/持久化与全部既有固定单品、成员、洗护、生命周期校验原样生效；锚点因而是硬约束而非可被画像绕过的软提示。输出意图带持久 `pairing` 标记（schema_version 1 + anchor_id）与 `creates_wear:false` 常量；`pairing.list` 按意图ID倒序键集分页读取带标记的历史，标记与生成控件不一致按存储损坏报错，查询不读库存与洗护状态，锚点失效后历史仍可读。学习上下文仅作解释性来源（`hard_constraints_override` 只是 context 字段），固定/排除等硬约束始终由共享推荐路径裁决。

配对排序决策：通过硬约束后的候选组合以配对奖励为第一排序键，既有单品秩（rank）仅作其后的并列比较；不把配对奖励并入原秩，也不插入旧 target/occasion 等软优先级之后逐项加权。配对排序采用精确惰性最佳优先检索（pair_search），处理非单调配对奖励：已选组合按资格感知的容许上界扩展（槽位对稀疏预计算最大已学边、已选ID向未决槽取实际最大非负边、未决负边只降低真值），不假设奖励单调，也不预先物化候选笛卡尔积。检索与持久化无关，调用方提供真实合格单品。

重置与删除边界：`learning.reset` 立即遗忘配对——管理发布直接移除画像上的 `pair_projection`；`learning.delete` 的管理发布同样临时移除配对投影，直到显式重算按当前记忆边界恢复，`include_prior=true` 完整重建才重新纳入此前历史（learning.rs 管理发布分支；learning.edit 分支仅追加显式修订，不影响配对投影）。

成员与来源边界：配对投影按成员隔离，权重只从当前成员画像读取；投影 schema/算法版本、有序ID、权重范围损坏按存储错误拒绝而非静默空。来源版本变化后的画像标记 `source_stale`，配对权重读取返回空集合——过期画像不参与排序，须显式重算；重置/记忆边界与 `include_prior` 语义沿用本 ADR 既有决定。

源码追溯：[learning.rs](../../crates/wardrobe-core/src/learning.rs)（`pair_projection_from_events`、`pair_event_weight`、`learning_pair_weights`、管理发布分支）、[pairing.rs](../../crates/wardrobe-core/src/pairing.rs)（`pairing_execute`、`pairing_list`）、[pair_search.rs](../../crates/wardrobe-core/src/pair_search.rs)（`PairSearch` 界与上界）；辅助：[recommendations.rs](../../crates/wardrobe-core/src/recommendations.rs) 共享生成入口。

实现边界：本补充仅基于 Core 源码审查，覆盖 CLI/Core 已实现的 pairing.generate/list、effective-pairs-v1 与配对检索；不覆盖 GUI 验收，不等于全 #19 完成。画像排序质量没有任何统计准确性声明（与本 ADR 既有约束一致）。
