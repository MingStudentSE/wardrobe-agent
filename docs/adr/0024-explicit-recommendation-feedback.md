# ADR 0024 · 显式推荐反馈与真实穿着分离

状态：采用，#17，F11/F30，DB19。

`feedback_events` 追加不可变修订，`feedback_receipts` 按成员和操作标识去重；更正/撤销以 expected_revision 比较最新修订。历史保留推荐意图ID、方案序号、原方案角色/层次、控制与反馈记录时观察到的库存快照，明确 `inventory_observed_at`；它不冒充更早的推荐生成时完整衣物事实。分页 list 提供当前修订，history 提供全部修订，不以最近50条代替完整历史。

喜欢、不喜欢、跳过、当天排除与三维可选1–5整数是明确反馈，空值不填3，不生成推断画像。当天排除选择方案内实物ID，记录IANA时区、当地日期、次日边界expires_at。现有共同planner在当前事务读取有效日排除和未撤销的不喜欢整套集合：日排除不改生命周期/洗护状态，固定与日排除冲突明确拒绝；不喜欢排除准确实物集合，不将拒绝推断成永久拒绝每件衣物或风格。用户可更正或撤销反馈重新允许组合。

Try Another 先幂等保存skip，再使用已有recommendation.change创建子意图；真实固定/排除、保留角色、当前洗护可用性和AI再次发布校验沿用共同实现。若换套失败，skip已经保存，错误明确说明，重试不重复反馈。两个操作分别有稳定派生操作标识；AI子意图仍返回 durable job receipt，通过同一worker契约处理。受限客户端需同时拥有feedback.try_another、feedback.record与recommendation.change。

实际穿了/改穿要求wear.confirmed=true、明确当地日期/时区；实际穿了使用原方案ID，改穿必须明确不同实物组合。反馈调用现有WearEvent事务写入器，与反馈收据在一个IMMEDIATE事务内提交，不自行维护穿着计数。受限客户端还需wear.record授权。反馈评分更正或反馈撤销不修改已发生的WearEvent；穿着错误由既有wear.correct/revoke独立处理，UI明确给出该区别。

`feedback_asset_refs` 只保留反馈快照所选展示图ID；修订/撤销不删除历史引用。assets.detach和sync_manifest共同保留历史曾采用规范化商品图，既有mime/state/scope限制继续排除原图、预览、人物和试穿。全module迁移实际建表且数据库max19，版本较高时亦修复缺表。后续同步/备份应采用显式反馈事实和展示引用白名单，而非dump本机providers/jobs/凭据。

GUI沿用Atelier、tokens和推荐页，提供空评分、独立意见、当前日期排除、明确穿着确认与分页历史；CLI/MCP通过相同Application descriptor，权限不能由复合操作扩大。验收证据与实际限制见ticket-17验证报告。
