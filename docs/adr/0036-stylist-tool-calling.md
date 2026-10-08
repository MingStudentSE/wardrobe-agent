# ADR 0036: 内置 Stylist 工具调用（只读工具环与受控写工具）

日期：2026-10-07。状态：已实现（今日穿搭 / 库存查询 / 分析；用户明确指令下的受控记录：穿着、洗护、单品编辑/归档）。关联 issue：#22（父 #1，F17）；婚礼/面试、旅行打包与购买适配场景归 #23/#24，本 ADR 只固化工具层与今日/查询/分析/受控记录边界。

## 背景

内置 Stylist 让用户用自然语言完成今日穿搭、库存查询与衣橱分析。核心风险是让模型直接执行任意能力：直接 SQL、任意文件/网络、越权成员、把未知当事实。父规格已定：Agent 使用领域工具、不直接 SQL 或任意执行；统计数字来自确定性查询，未知事实明确未知；模型失败有限重试后如实反馈。

## 决策

1. **复用 operations 注册源，不建第二张调度表。** `stylist.ask` 与 `stylist.worker.run` 是注册表里的普通 operation，带闭合 input/output schema、scope 元数据（由授权规则同源派生）与 effect/examples。CLI、MCP、GUI 三入口经同一分发面调用，不新增入口代码路径。
2. **工具白名单，服务端持有参数。** 只读面六工具：`clothing.list`（库存分页）、`recommendation.generate`（今日穿搭，强制本地规则路径）、`recommendation.list`（最近推荐意图）、`insights.summary`（确定性统计）、`wear.list` 与 `care.list`（写后回读与更正/撤销的 grounding）。受控写面六工具：`wear.record`（实际穿着，含幂等键）、`care.record` / `care.correct` / `care.revoke`（洗护）、`clothing.update`（name/notes/favorite/seasons 白名单字段）、`clothing.archive`。模型帧只允许出现白名单操作名；参数由服务端从帧中按字段白名单重建，策略、供应商、operation_key、日历日期与时区永远由服务器注入。模型把 `strategy:"ai"` 塞进工具帧会被静默剥离并按规则生成，不能借 Stylist 的任务外花供应商额度。
3. **任务收据先行，账本驱动。** `stylist.ask` 校验输入与模型配置后先持久化 `stylist_query` 任务并立即返回 queued 回执，模型 HTTP 调用只发生在 `stylist.worker.run`。账本记录每次模型轮次（submitted/received/invalid_output/used）与每次工具调用（call_id、operation、参数、投影结果），重放终态任务不再产生任何 POST。
4. **预算与有限重试。** 模型轮次上限 8、工具调用上限 6；模型输出不合法（非指定 JSON、未知字段、引用未经投影路径）计 invalid_outputs，初次加两次重试后以 `model_invalid_output` 如实失败；限流按 Retry-After 最多自动重试 2 次后排队；提交结果未知（连接失败、无响应字节）进入 `waiting_input`，提交账本阻止盲重发与显式 retry 重花额度（`submission_already_sent`）。
5. **投影面。** 工具结果进入提示词前经服务端投影：只保留安全字段（id/name/category/status、推荐项、统计聚合），备注、controls、learning/feedback 上下文等敏感字段从不进入模型提示词；封顶数组（stale/never_worn 24、推荐意图 12、组合 12）一律附带服务器计算的精确总数，封顶页不得被当作全局总数。回答中的事实引用（JSON pointer）由服务器从已投影结果取值（`facts[].value`），永不信任模型自报的值；`explanation` 标记 `explanation_verified:false`，只是未经核验的解释。
6. **今日穿搭经真实推荐域；写动作只在用户明确指令下发生。** 只读查询调用共享 `recommendation.generate`：真实库存、硬约束、固定/排除、洗护状态全部由既有推荐域裁决；为幂等，服务器按 (任务ID, 调用槽) 注入 `operation_key=stylist-{job}-{call_id}`，同槽重放返回同一意图，不产生重复意图。生成意图是领域工件（`creates_wear:false`），不是穿着事实。受控写工具遵守同一确定性幂等键注入：同一任务内与已执行写调用完全相同的模型字段被视为同一指令的重复帧，按重放处理（标记轮次已用、不再分发领域写入、不再新增账本调用），模型重试绝不重复计数；跨任务仍是不同指令、各自成账。写面语义红线：保存搭配、接受推荐、生成计划绝不变成穿着或洗护——`recommendation.save`、`feedback.record` 等隐式面不在白名单；`wear.record` 只由用户问题的明确指令（如「记一下今天穿了X」「这件洗过了」）映射触发，且该要求写入系统提示词并由账本完整留痕（问题、工具帧、注入参数、回执可审计）。`care.correct`/`care.revoke` 要求 `root_event_id` + `expected_revision`，模型必须先经 `care.list` 投影引用真实修订号，凭空引用按 `not_found`/`version_conflict` 如实失败。
7. **分析经确定性统计。** `insights.summary` 的 as_of_date 由服务器按任务时区的“今天”注入；数字（总数、分布、久未穿、从未穿）全部来自确定性查询，未知明确未知。
8. **授权：存储来源 + 延迟重放校验。** 任务载荷不可变地记录 `StylistOrigin`（TrustedLocal 或 Client{grant_id}）。客户端来源的每一次工具分发（读或写）经 `execute_as_grant_id` 逐工具重放 scope 与成员校验：limited 客户端未授权 `insights.summary` 则该工具调用以 `scope_denied` 失败，未授权 `wear.record` 则写入被拒且不产生任何业务副作用；撤销/到期在每次 worker 推进时重新生效，撤销后的排队任务连可信 worker 也拒绝（`authorization_revoked`）。其他客户端不能 run/cancel/retry 别人的 stylist 任务（scope_denied）。
9. **写面即既有领域能力的受控入口，不是第二套业务。** 写工具直接分发到共享领域操作（wear/care/clothing 的既有校验、乐观并发、审计与 outbox 全部原样生效）；Stylist 只贡献：确定性幂等键注入、成员/日期的服务器持有、字段白名单与投影。领域校验失败（如回收站单品不能记穿着）按原错误码如实反馈给用户。
10. **场景边界。** 婚礼/面试、旅行打包与购买适配不在本片；`clothing.delete`/`restore`（回收站）刻意不入模型工具面——删除类动作保留给专门 UI 入口。

## 后果

- 模型幻觉被结构性限制：引用必须解析到真实投影，数字来自服务器，封顶页带总数，解释显式未核验。
- 工具面与 operations 注册单一来源一致；GUI/CLI/MCP 能力发现自动包含 stylist 操作。
- 真实模型成功需用户自配 OpenAI-compatible 端点另行实测；协议行为由本地脚本化 HTTP oracle 覆盖（测试边界沿用 2026-10-07 用户确认：确定性场景替换不可控模型）。
- 已知限制：模型参数错误（如非法 limit）会计入 invalid 重试或如实失败，不静默纠正；天气不由模型直接调用，自动天气仍由推荐域内部解析。

源码追溯：[stylist.rs](../../crates/wardrobe-core/src/stylist.rs)、[stylist_queue.rs](../../crates/wardrobe-core/src/stylist_queue.rs)、[stylist_runtime.rs](../../crates/wardrobe-core/src/stylist_runtime.rs)、[provider_text.rs](../../crates/wardrobe-core/src/provider_text.rs)；测试 [tests/stylist.rs](../../crates/wardrobe-core/tests/stylist.rs)、[CLI/MCP 验收](../../../docs/verification)。
