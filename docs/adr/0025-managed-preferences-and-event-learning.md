# ADR0025 · 可管理偏好与事件学习

状态：实现中，#18/F18，DB20。

显式偏好与推断画像分开。允许color/style/material/pattern/category/occasion特征，不接受尺寸、身体或衣物事实写入。显式偏好为用户声明，带说明、不可变修订与CAS；推断来自有效反馈和实际WearEvent当时快照，不读取当前衣物字段代替历史。用户编辑推断即创建明确覆盖，同特征明确值优先于推断，不修改原事件。

算法explicit-and-events-v1：每个事件内相同特征只计一次。like +2、dislike -2、skip/day_exclude/actual_wore/changed反应本身0；真实有效穿着事件+1。总体评分N-3作用于全部已知特征；舒适N-3仅作用material；风格N-3作用color/pattern/style。空评分0，无未知特征猜测。累计特征权重限制-5至5。来源完整保留有效反馈修订ID、有效穿着修订ID，事件更正/撤销后重算读取当前有效最新修订。相同源集合得相同有序信号与权重；新画像有自己的ID/时间。它是透明排序启发式，不能宣称统计预测准确度。

推荐在既有目标/场合等优先级之后用特征权重排序候选，共享规则组合与模型发布前再次检查当前固定/排除、反馈/可用性。画像不会绕过硬约束；既有推荐意图继续不可变，新显式推荐使用新操作标识。推荐context带算法版本和画像来源，用户可解释。

删除特征持久化suppression并立即移除当前影响；重置推断写入UTC记录时间边界，普通重算只读取边界之后记录的新有效事件。重置全部还追加显式偏好删除修订。事实事件永不删除。用户明确include_prior=true完整重建才清除suppression/边界并重新纳入此前历史，UI明确解释与确认，不由后台自动发生。管理历史保留修改审计，不让旧偏好重新生效。

迁移实际创建learning_memories、learning_profiles、learning_state、learning_history、learning_receipts，无条件修复缺表，数据库max20。成员与客户端scope由共同Application入口执行；GUI/CLI/MCP同一descriptor。业务同步/备份白名单需包含管理边界与历史及来源，禁止携带providers/jobs/credentials；协同后续模块统一serializer，不另写catchall导出。
