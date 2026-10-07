# ADR 0030: 购买候选域与购买适配分析

日期：2026-10-07。状态：已实现。关联 issue：#24（父 #1，F17 购买场景）；工具层边界沿用 [ADR 0026 内置 Stylist 工具调用](0026-stylist-tool-calling.md)，识别/纠错管线沿用 #12 的产品化变体。

## 背景

用户在购买前希望知道：候选商品与现有衣橱能配出什么、是否与已有单品高度重复、是否填补衣橱空洞。核心风险与 #12/#22 同源：把候选混入真实库存（污染统计与推荐）、把模型建议当事实（编造尺码/价格）、以及引入"自动购买"类越权动作。

## 决策

1. **候选域独立于衣物身份。** `purchase_candidates`/`purchase_analyses` 两张本机表，候选文档带 `domain=purchase_candidate` 与独立 UUIDv7；不写 clothing/audit/outbox，因此结构性不进入库存、穿着、洞察、推荐、学习触发器、同步与便携备份（恢复时随识别建议/导入草稿一并清除，见 backup 还原的本地表清单）。候选不能被 wear/clothing 系列操作引用（外键层面即不存在）。
2. **识别是 #12 管线的产品化变体，不是第二个识别系统。** `purchase.candidate.recognize/process` 复用任务收据先行、明确上传确认、图片快照哈希校验、租约边界落账、提交未知不盲重发（含恢复后的 submission guard）等既有机制；任务 kind 为 `purchase_recognition`。建议只写入候选 provenance（suggested_facts + uncertainties），人工纠错经 `purchase.candidate.update` 显式发生（confirm_correction），人工值优先。规范化复用 recognition 的保守词表映射，提示词版本 `candidate-visible-description-v1` 明确禁止猜测品牌/尺码/材质/价格。
3. **分析是服务端确定性计算，只读真实库存。** `purchase.analysis.run` 只读 active 且洗护可用的库存（排序取 ID 序），三节全部由固定规则机械产生：搭配潜力复用推荐域角色/覆盖规则（候选固定占据其角色槽位，组合总数=覆盖槽位库存数的精确乘积，展示≤5 套带精确总数）；重复风险按同类别逐项 Jaccard 加权（颜色400/风格250/场合200/季节150，千分制整数运算；≥700 且≥2 维度可比记高度重合；任一方未记录的维度跳过并列入 unknown_dimensions）；缺口分析按内置类别与四场景计数，0 即空洞，候选"填补"声明只在类别/场景与空洞精确相等时作出。每次运行产生新版本并全部保留（`version` 递增），历史可查可交叉核对真实库存 ID。
4. **不确定性显式化。** 候选缺类别→搭配角色与重复风险不可用（不猜测）；缺颜色/风格等→对应维度跳过并说明可能低估；自定义类别单列且不参与内置空洞判定；库存未记场合标签计数说明；分析文档恒定携带 `creates_wear:false`、`counts_toward_inventory:false`、`auto_purchase:false`。能力面不注册任何 buy/checkout/cart/pay 语义操作。
5. **三入口经同一注册源。** 十个 `purchase.*` 操作以普通 descriptor 进入 operations()（scope 由授权规则同源派生，全部 external/per-member_id），CLI/MCP 经共享分发面自动获得；GUI 在穿搭工作区新增「购买分析」页签，遵循既有视觉系统、中文文案与键盘可达性。

## 后果

- 候选与库存的区分是结构性的（不同表、不同身份、无外键通道），测试以库存统计/推荐组合/穿着记录不变式钉住。
- 分析质量上限取决于候选与库存的事实完整度；缺数据时结论降级为"不可用+原因"，绝不补猜。
- 已知限制：组合枚举仅展示前 5 套（总数精确）；重复比较限于同类别（跨类别替代不作判断）；场景覆盖只统计已记录标签。

源码追溯：[purchase.rs](../../crates/wardrobe-core/src/purchase.rs)、[jobs.rs](../../crates/wardrobe-core/src/jobs.rs)、测试 [tests/purchase.rs](../../crates/wardrobe-core/tests/purchase.rs)、[CLI 契约](../../crates/wardrobe-cli/tests/purchase.rs)、[GUI spec](../../apps/desktop/tests/purchase.spec.ts)。
