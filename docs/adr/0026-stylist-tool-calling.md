# ADR 0026: 内置 Stylist 工具调用（只读工具环）

日期：2026-10-07。状态：已实现首片（今日穿搭 / 库存查询 / 分析）。关联 issue：#22（父 #1，F17）；婚礼/面试、旅行打包与购买适配场景归 #23/#24，本 ADR 只固化工具层与今日/查询/分析边界。

## 背景

内置 Stylist 让用户用自然语言完成今日穿搭、库存查询与衣橱分析。核心风险是让模型直接执行任意能力：直接 SQL、任意文件/网络、越权成员、把未知当事实。父规格已定：Agent 使用领域工具、不直接 SQL 或任意执行；统计数字来自确定性查询，未知事实明确未知；模型失败有限重试后如实反馈。

## 决策

1. **复用 operations 注册源，不建第二张调度表。** `stylist.ask` 与 `stylist.worker.run` 是注册表里的普通 operation，带闭合 input/output schema、scope 元数据（由授权规则同源派生）与 effect/examples。CLI、MCP、GUI 三入口经同一分发面调用，不新增入口代码路径。
2. **只读工具白名单，服务端持有参数。** 首片白名单四工具：`clothing.list`（库存分页）、`recommendation.generate`（今日穿搭，强制本地规则路径）、`recommendation.list`（最近推荐意图）、`insights.summary`（确定性统计）。模型帧只允许出现白名单操作名；参数由服务端从帧中按字段白名单重建，策略、供应商、operation_key、统计日期与时区永远由服务器注入。模型把 `strategy:"ai"` 塞进工具帧会被静默剥离并按规则生成，不能借 Stylist 的任务外花供应商额度。
3. **任务收据先行，账本驱动。** `stylist.ask` 校验输入与模型配置后先持久化 `stylist_query` 任务并立即返回 queued 回执，模型 HTTP 调用只发生在 `stylist.worker.run`。账本记录每次模型轮次（submitted/received/invalid_output/used）与每次工具调用（call_id、operation、参数、投影结果），重放终态任务不再产生任何 POST。
4. **预算与有限重试。** 模型轮次上限 8、工具调用上限 6；模型输出不合法（非指定 JSON、未知字段、引用未经投影路径）计 invalid_outputs，初次加两次重试后以 `model_invalid_output` 如实失败；限流按 Retry-After 最多自动重试 2 次后排队；提交结果未知（连接失败、无响应字节）进入 `waiting_input`，提交账本阻止盲重发与显式 retry 重花额度（`submission_already_sent`）。
5. **投影面。** 工具结果进入提示词前经服务端投影：只保留安全字段（id/name/category/status、推荐项、统计聚合），备注、controls、learning/feedback 上下文等敏感字段从不进入模型提示词；封顶数组（stale/never_worn 24、推荐意图 12、组合 12）一律附带服务器计算的精确总数，封顶页不得被当作全局总数。回答中的事实引用（JSON pointer）由服务器从已投影结果取值（`facts[].value`），永不信任模型自报的值；`explanation` 标记 `explanation_verified:false`，只是未经核验的解释。
6. **今日穿搭经真实推荐域。** 工具环调用共享 `recommendation.generate`：真实库存、硬约束、固定/排除、洗护状态全部由既有推荐域裁决；为幂等，服务器按 (任务ID, 调用槽) 注入 `operation_key=stylist-{job}-{call_id}`，同槽重放返回同一意图，不产生重复意图。生成意图是领域工件（`creates_wear:false`），不是穿着事实。
7. **分析经确定性统计。** `insights.summary` 的 as_of_date 由服务器按任务时区的“今天”注入；数字（总数、分布、久未穿、从未穿）全部来自确定性查询，未知明确未知。
8. **授权：存储来源 + 延迟重放校验。** 任务载荷不可变地记录 `StylistOrigin`（TrustedLocal 或 Client{grant_id}）。客户端来源的工具分发经 `execute_as_grant_id` 逐工具重放 scope 与成员校验（如 limited 客户端未授权 `insights.summary` 则该工具调用以 `scope_denied` 失败）；撤销/到期在每次 worker 推进时重新生效，撤销后的排队任务连可信 worker 也拒绝（`authorization_revoked`）。其他客户端不能 run/cancel/retry 别人的 stylist 任务（scope_denied）。
9. **写工具不在本片范围。** “用户明确指令下记录穿着/洗护/编辑”要求工具环支持受控写操作；为不越界实现 #23/#24 场景，本片先固化为只读环。受控写工具（wear.record、care 记录、clothing 编辑）按同一账本与授权模式在后续片加入，非本 ADR 变更。

## 后果

- 模型幻觉被结构性限制：引用必须解析到真实投影，数字来自服务器，封顶页带总数，解释显式未核验。
- 工具面与 operations 注册单一来源一致；GUI/CLI/MCP 能力发现自动包含 stylist 操作。
- 真实模型成功需用户自配 OpenAI-compatible 端点另行实测；协议行为由本地脚本化 HTTP oracle 覆盖（测试边界沿用 2026-10-07 用户确认：确定性场景替换不可控模型）。
- 已知限制：模型参数错误（如非法 limit）会计入 invalid 重试或如实失败，不静默纠正；天气不由模型直接调用，自动天气仍由推荐域内部解析。

源码追溯：[stylist.rs](../../crates/wardrobe-core/src/stylist.rs)、[stylist_queue.rs](../../crates/wardrobe-core/src/stylist_queue.rs)、[stylist_runtime.rs](../../crates/wardrobe-core/src/stylist_runtime.rs)、[provider_text.rs](../../crates/wardrobe-core/src/provider_text.rs)；测试 [tests/stylist.rs](../../crates/wardrobe-core/tests/stylist.rs)、[CLI/MCP 验收](../../../docs/verification)。
