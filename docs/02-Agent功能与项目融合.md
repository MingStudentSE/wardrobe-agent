# wardrobe-agent Agent 功能与项目融合

设计草案 1.0，2026-10-06。证据见三个 `*-source-audit.md`，对应本轮下载的固定 commit。本文为推荐产品设计，不代表已实现软件。

## 1. 最终 Agent 的定位

一个能读取真实衣橱、调用业务工具、生成可执行穿搭方案、管理图片任务并从真实反馈改进建议的个人造型助理。它既有内置 Stylist 聊天入口，也可由外部 Agent 经 MCP/CLI 使用；用户可以关闭内置 AI，继续手动工作和外部工具工作流。

以一个逻辑 Agent + 领域工具 + 持久工作流为首选，不先创建多个互相聊天的“造型/天气/记忆 Agent”。识别、推荐、图片生成是可独立调用的服务和任务。这样能集中控制成员范围、数据来源、成本、取消和审计，也能让 GUI/CLI/MCP 行为一致。

## 2. Agent 能做什么

| 功能组 | 具体能力 | 工具/数据依赖 | 完成条件 |
|---|---|---|---|
| 衣橱理解 | 搜索衣物，查询颜色/材质/季节/状态，解释已有库存 | items.search/get、assets、profile | 回答引用单品 ID；未知字段明确说未知 |
| 智能录入 | 从照片建议标签、抠图、商品图、批量待审核录入 | items.import、vision.analyze、images.process/generate | 草稿可改、原图保留、保存后产生实体与变更日志 |
| 今日穿搭 | 天气/场合/偏好/历史/洗护状态生成多套可执行方案 | context、recommendations、wear、care | 校验所有 ID 存在且可用，显示理由及不确定条件 |
| 围绕单品搭配 | 固定一件，寻找配对；替换鞋/外套；锁定部分换一套 | pairings、constraints、recommendations | 必选不被偷偷替换；无解给出原因 |
| 手动搭配辅助 | 选衣、画布/拼贴、Lookbook、克隆修改、保存 | outfits.create/update、assets.compose | 不依赖云模型；保存不自动记穿着 |
| 场景计划 | 婚礼、面试、旅行多日计划，天气变化、减少携带重复 | user goal、forecast、outfit plan | 从真实库存出方案；旅行打包是新增能力，不冒称参考库已实现 |
| 视觉表现 | 白底单品图、整套 flat-lay、试穿、前后/多套对比 | image provider、jobs、asset lineage | 返回任务/结果，保留原图及输入；生成图片需用户采用 |
| 真实穿着 | 用户表达“今天穿这套”后记录；补记/改穿/撤销 | wear.record/amend/retract | 一次操作一个去重事件，不因重试重复计数 |
| 洗护提醒 | 查询待洗、记录清洗、清洗后恢复可用 | care.record/status、events | 依据穿着/洗护事件计算，而非同步累加计数 |
| 反馈学习 | 喜欢/拒绝、舒适/风格评分、实际替代方案 | feedback、preference/learning | 显式偏好与推断分开，用户可重置/编辑 |
| 衣橱分析 | 长期未穿、频率、色系、组合、缺口/重复购买风险 | deterministic analytics、wear events | 数字来自查询，LLM 只解释；统一时区与口径 |
| 购买适配 | 新商品图与现有衣橱搭配潜力、是否重复、需补哪些类别 | external item draft、pairing、inventory | 区分“衣橱已有”与“候选购买”，不伪造真实尺寸或自动买东西 |
| 定时助理 | 今天/前夜推荐、洗护提醒、失败重试和通知 | local scheduler、jobs、notifications | 调度去重；休眠恢复策略明确；不保证关机时执行 |
| 独立成员档案 | 切换主人、独立偏好与历史 | profile scope | 由同一用户管理，不提供邀请、共享、协作角色或家人评价 |
| 数据管理 | 查询同步状态、冲突提示、导出、恢复预览 | sync/status、backup | AI 不决定同字段事实冲突，不擅自永久删除 |

日历不是必需前置条件：用户可手选场合；启用日历后由 ContextService 给出相关活动摘要，不默认把全部日历原文上传或同步。

## 3. 三项目各吸收什么

| 来源 | 继承的优点 | 最终对应能力 | 必须重做/纠正 |
|---|---|---|---|
| Wardrowbe | 完整衣橱、多图、批量幂等与恢复、洗护 | 录入与维护闭环、可靠任务、CareService | 服务器队列改本地持久队列；删除改 tombstone |
| Wardrowbe | 规则筛选 → LLM 组搭 → ID/角色校验 | 推荐引擎、单品配对、固定/排除约束 | 加组合级评分、跨端一致事件、离线兜底 |
| Wardrowbe | Studio/Lookbook、推荐与实际改穿区分 | 手工搭配、模板、WearEvent/Feedback | 不以接受或保存搭配直接记穿着 |
| Wardrowbe | 显式偏好/学习画像分层、配对学习 | 个性化排序、可解释记忆 | 派生画像从事件重算、版本化、可重置 |
| Wardrowbe | 时区、前夜推荐、通知机制 | 个人计划与系统通知 | 处理休眠恢复；不继承家庭协作与多渠道协作通知 |
| Wardrowbe | 能力发现、关闭内置 AI、外部写回 | capabilities、独立 CLI/MCP | HTTP API 不能直接算 MCP；统一 schema 与写回验证 |
| AIWardrobe | 上传→抠图→识别→纠错，简洁操作 | 智能录入向导、快速编辑 | AI 失败不阻断手工保存；先保存原图 |
| AIWardrobe | rembg/remote fallback，批量单张失败隔离 | 本地 CV 与持久任务 | 不让前端触发 pip install；受控模型下载 |
| AIWardrobe | 场景目标、可解释规则、已有/补购分开 | 目标优先/库存优先、推荐解释 | 非逐类独立最高分，补全整套协调与历史 |
| AIWardrobe | OpenAI-compatible Vision、自定义试穿 adapter | 能力化 provider 配置 | 不把所有 key 明文 JSON；星座默认不进入事实推荐 |
| ClosetAI | 原图/衍生图分离、方向归一化、本地抠图 | 媒体资产管线、纯白展示 | 本地白底缩放不冒称“服装展平重建” |
| ClosetAI | 细分类别、多选季节场合、照片与标签联动 | 更完整的分类和编辑 UX | 季节 canonical 值为四季多选，“四季”转为四项 |
| ClosetAI | 多图 flat-lay、试穿对比、本地拼贴兜底 | 图像生成模块、试穿模块 | provider 输入限制先校验；不静默 prefix 截断 |
| ClosetAI | Keychain、软删除恢复 | 安全凭据、回收站 | 穿搭也软删除；统计必须用真实穿着事件 |

## 4. 新增能力与非继承部分

真正的工具调用 Agent、独立 CLI/MCP、统一 Rust 业务核心、S3-compatible/WebDAV/local-folder 增量同步、E2EE、设备配对与冲突 UI、可恢复图片作业、移动共享协议都需要自行实现。三个项目均不提供完整的目标架构。

星座属于 AIWardrobe 的娱乐偏好，未见用户要求；默认不继承为核心决策。iOS 26 限定、NextAuth、Docker/Redis/PostgreSQL 不直接继承。家庭邀请、角色权限、共享协作与家人评价明确不纳入产品；个人定时推荐与系统通知保留。

## 5. Agent 执行契约

1. **获取上下文**：成员、地点/天气、时间、用户目标、已授权日历摘要、库存、穿着/洗护、明确偏好。
2. **计划与读取**：LLM 调用只读工具；库存与统计由查询提供，不靠模型记忆猜测。
3. **候选与约束**：确定性引擎处理可用性、温度、场合、层次、必选/排除、近期重复；模型从候选组织多套方案。
4. **校验**：结构化输出验证、实际 ID 存在、所属成员、类别覆盖、同部位冲突与必选要求；非法输出重试有限次后规则回退。
5. **展示与落地**：展示衣物/理由/替代项；用户可保存/改穿/生成图/记录实际穿着。读、写、上传、付费生成分别有明确策略。
6. **记账与学习**：保存领域事件和操作来源；画像可重新计算。工具重试使用 idempotency key。

## 6. 首版工具集合

```text
capabilities.get
profiles.list / get / select
items.list / search / get / import / update / archive / restore
assets.get / list
outfits.list / get / create / update / clone / archive / restore
lookbook.list / save / apply
pairings.recommend / submit
recommendations.generate / get / replace_item / submit
wear.list / record / amend / retract
care.get_status / record_wash / amend
feedback.record / list
preferences.get / update
learning.get / reset
analytics.summary / usage / unused / colors / pairings
context.get / weather.get
images.remove_background / product / outfit / try_on
jobs.get / list / cancel / retry
schedules.list / create / update / disable
sync.status / run / conflicts.list / conflicts.resolve
backup.export / restore_preview
```

这是内部逻辑名称；MCP 导出时使用兼容 SDK 的稳定工具名（如 `wardrobe_items_search`），不强制沿用聊天中带点名称。所有工具有 input/output schema、member_id、分页、类型化错误；长任务立即返回 job_id。写工具按客户端 granted scopes 执行。永久清理、自动照片上传与费用消费不默认开放给外部 Agent。

Resources：`wardrobe://profiles/{id}/items`、`wardrobe://items/{id}`、`wardrobe://outfits/{id}`、`wardrobe://history/{month}`、`wardrobe://analytics`、`wardrobe://preferences`；大列表分页，媒体按范围请求，不一次倾倒全部照片。

## 2026-10-07 图片同步约束

Agent 采用统一图片管线：原图 local-only，最终商品图压缩后才可同步；无需 AI 的图片同样压缩。商品图以外的图片不自动同步，工具/对话附件不能绕过白名单。其他设备无法读取导入设备原图，重新生成需回到原设备或重新导入。
