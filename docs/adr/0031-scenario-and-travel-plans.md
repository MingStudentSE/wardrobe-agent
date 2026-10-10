# ADR 0031: 场景与旅行计划（婚礼/面试、多日旅行与去重打包清单）

日期：2026-10-07。状态：已实现。关联 issue：#23（父 #1，F17）；架构基线沿用 ADR 0036（内置 Stylist 工具环）。

## 背景

婚礼/面试与多日旅行需要基于真实库存的逐天搭配和打包清单，且计划是安排而不是穿着事实。风险与 #22 相同：不能让计划变成 WearEvent，不能虚构库存，不能让模型绕过硬约束或花任务外额度。#22 已确立：operations 单一注册源、服务端持有参数、收据先行、有限重试、提交账本防盲重发。

## 决策

1. **复用 #22 的结构性红线：计划绝不产生穿着。** `scenario.plan/get/list/update/delete/worker.run` 是注册表里的普通 operation（闭合 input/output schema、scope 由授权规则同源派生）。计划文档 `creates_wear` 恒为 false；场景路径不触碰 wear/care 写面；测试把「wear.list 为空」作为外部行为断言。
2. **计划与打包清单是确定性领域工件。** 逐天方案复用推荐域规划器（`recommendations::plan_single`，同一角色/覆盖/低温外层规则），跨天规则：贴身/主要衣物类别（top、bottom、dress、one_piece、sportswear、underwear）不连续两天重复，鞋/外层/包/配饰可跨天复用。打包清单按衣物聚合去重，`reused`/`consecutive_repeat` 如实标记；库存不足时允许连续穿着降级并把原因写进 shortage_reasons，绝不虚构或静默重复。
3. **库存与归属硬校验在服务端。** 候选池 = active 生命周期 ∩ 洗护可用（care_batch 投影）− 显式排除；固定单品逐个 clothing_get 校验归属 + active + 可用，违反按 `constraint_conflict`/`not_found` 如实拒绝。修改计划（expected_version 乐观锁）显式排除自动解除同单品固定，避免固定/排除死锁。
4. **天气参考可选、失败降级。** 复用 #16 天气能力（手动温度/已配置地点自动获取）；未来日期无可靠预报时如实标注 unavailable 并不强制外层，网络失败写 last_error 不阻断；手工说明（notes）随计划保存，作为人工补充。
5. **模型辅助走 #22 的 provider 文本通道与账本语义。** strategy=ai 先落 scenario_plan 任务并返回回执；worker 先本地规则生成候选（同时是降级结果与校验基线），再单轮请求模型整理逐天指派。模型输出按服务端结构规则逐天校验（真实ID/归属可用/固定落位/逐天轮换/基础覆盖/天序完整），失败最多三次尝试后按本地规则降级，`generation` 记录 source=rule_fallback、失败码与 model_feedback；提交账本（submitted/invalid_output/rate_limited）与 text_recommendation 同语义，提交结果未知进入 waiting_input，jobs.retry 以 `submission_already_sent` 拒绝盲重发。真实模型成功需用户自配 OpenAI-compatible 端点另行实测；协议行为由本地脚本化 HTTP oracle 覆盖（沿用 2026-10-07 用户确认的测试边界）。
6. **持久化与可移植边界。** 计划存 SQLite `scenario_plans`（UNIQUE(member_id, operation_key) 幂等）。便携备份的 archive 形状是显式版本化契约（见 backup.rs），本片不扩展备份形状——计划是本机持久事实但暂不进入便携备份/同步，作为已知限制记录，后续形状版本演进时一并纳入。

## 后果

- 场景操作经共享注册源自动暴露 CLI/MCP/GUI（场景计划 tab 落在 outfits 工作区「问问衣橱」旁），不新增第二张调度表。
- 已知限制：计划不入便携备份与同步（见决策6）；模型辅助为单轮整理（不是 #22 的多轮工具环），复杂重排能力以本地规则为上限；GUI 场景入口只覆盖规则路径与 AI 收据驱动，不提供模型失败后的显式重试按钮（可用新 operation_key 重新发起）。

源码追溯：[scenarios.rs](../../crates/wardrobe-core/src/scenarios.rs)、[scenarios_runtime.rs](../../crates/wardrobe-core/src/scenarios_runtime.rs)、GUI [ScenarioPlanner.tsx](../../apps/desktop/src/components/ScenarioPlanner.tsx)；测试 [tests/scenarios.rs](../../crates/wardrobe-core/tests/scenarios.rs)、[CLI/MCP 契约](../../../crates/wardrobe-cli/tests/scenario_entrypoints.rs)、[GUI spec](../../../apps/desktop/tests/scenarios.spec.ts)、[验收记录](../verification/ticket-23/report.md)。
