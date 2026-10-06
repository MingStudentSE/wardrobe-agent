# Wardrowbe 源码审计

审计日期：2026-10-06。仓库：<https://github.com/Anyesh/wardrowbe>。固定快照：`1f6f6f49150dc2cc4fe6d0b28460213e9d461e52`。本报告路径均相对 `references/wardrowbe/`；只做静态源码核对，未安装依赖、启动服务或执行测试，因此“已实现”指存在完整代码链路，不代表本机端到端验收成功。未发现仓库 AGENTS.md。

## 结论

三个参考项目中，Wardrowbe 应当承担衣橱产品的功能基线与领域建模参考。它已经超出“上传衣服 + 问 LLM 今天穿什么”：拥有穿着与洗护事件、手工搭配工作台、Lookbook、真实穿着反馈、单品配对学习、家庭权限、时区感知定时推荐和分析。它仍是依赖 PostgreSQL、Redis、后台 worker 的自托管 Web 系统，不是可直接改皮肤发行的本地桌面 Agent。

## README 宣称与源码证据

README.md:43–53 宣称照片衣橱、天气/场合/偏好推荐、通知、家庭、穿着反馈、分析、自托管、OpenAI 兼容模型和八语言。

| 能力 | 实际实现与边界 | 证据 |
|---|---|---|
| 单品录入 | 单件、批量上传；列表/筛选；编辑、收藏、归档/恢复、删除；图片旋转、去背景、恢复原图、替换主图、添加多图/重排/设主图 | `backend/app/api/items.py:127–287,517–1022,1243–1352,1702–2090` |
| AI 标签 | 类型、子类、颜色、图案、材质、风格、正式度、季节、版型、场合、品牌、状态、特征、描述；未知分类和原始响应保留；置信度不等于真实校准准确率 | `backend/app/services/ai_service.py:41–63`；`backend/app/workers/tagging.py:95–140,178` |
| 可靠录入 | IndexedDB 持久上传队列，记录失败/重试/终止状态；服务器每用户 upload_key 幂等；图片 pHash 字段 | `frontend/lib/upload-queue.ts:3–45,162–276`；`backend/app/models/item.py:59–64` |
| 日常推荐 | 天气/场合/偏好、必选/排除单品、多方案、Try Another 缓存、当天拒绝排除、最近穿着/推荐避免重复；推荐真实衣橱中的 ID | `backend/app/services/recommendation_service.py:90–123,125–218,690–957` |
| 规则 + LLM | 先天气/正式度/季节/近期穿着/偏好/低使用率评分、配对加成和角色多样性，再让模型组织方案；模型输出 ID 校验、去重、补必选、身体部位冲突去重 | `backend/app/services/item_scorer.py:381–445`；`backend/app/services/recommendation_service.py:584–638` |
| 单品配对 | 以一个单品为锚点生成/浏览配对，也允许外部作者写回配对 | `backend/app/api/pairings.py:228–388`；`backend/app/services/pairing_service.py:175–321` |
| 手工工作台 | 从零创建搭配、记录实际改穿、克隆为 Lookbook 模板、模板今天穿、编辑；已穿搭配不可随意改写历史 | `backend/app/services/studio_service.py:25–37,141–193,263–377`；`backend/app/api/outfits.py:1223–1435` |
| 反馈/历史 | 接受、拒绝、跳过、查看状态；总体/舒适/风格评分；是否真正穿过、修改、改穿单品；单品穿着次数/日期/历史 | `backend/app/models/outfit.py:27–42,155–180`；`backend/app/api/outfits.py:800–847,968–969`；`backend/app/api/items.py:1354–1470` |
| 洗护 | 次数阈值、待洗标记、洗护历史、洗护提醒；推荐常规候选排除待洗单品 | `backend/app/models/item.py:120–124,185–200`；`backend/app/services/studio_service.py:104–123`；`backend/app/workers/notifications.py:343–448`；`backend/app/services/recommendation_service.py:112` |
| 偏好学习 | 接受意图、实际穿着、评分分别成为信号，组合配对统计、画像增量更新/重算、风格洞察；这是规则统计学习，不是训练个性化神经网络 | `backend/app/services/learning_service.py:73–153,506–528,537–758,877–1078` |
| 家庭 | 家庭、邀请码/限时邀请、成员角色、加入退出/移除、家庭成员衣橱/搭配切换、搭配家人评分；用户归属和家庭权限检查存在 | `backend/app/models/family.py:15–60`；`backend/app/api/families.py:52–421`；`backend/app/services/outfit_service.py:181–199`；`backend/app/models/outfit.py:188–209` |
| 定时 | 星期、用户当地时间、场合、提前一晚通知、去重、异步发送/失败重试；按用户 ZoneInfo 转本地时间，支持次日预报 | `backend/app/models/schedule.py:15–35`；`backend/app/workers/notifications.py:120–340` |
| 多渠道 | ntfy、Mattermost、SMTP email 和 Expo push；渠道中立通知对象再由 provider 渲染 | `backend/app/services/notification_providers.py:21–27,44–73`；`backend/app/api/notifications.py:47–180` |
| 分析 | 总量/状态、周月搭配、接受率/平均评分、总穿着、颜色/类型分布、最常/最少/从未穿、接受率趋势、结构建议 | `backend/app/api/analytics.py:57–98,102–335` |
| 外部 Agent 接入 | 内部视觉/文字 AI 可关闭，capabilities 查询，外部 retag/suggestion/pairing 写回；是 HTTP API，并非现成 MCP Server | `backend/app/api/health.py:18–39`；`backend/app/api/items.py:1633–1652`；`backend/app/api/outfits.py:536–586`；`backend/app/api/pairings.py:321–385` |
| 多模型 | OpenAI 兼容 API，多端点启用/禁用、fallback、超时重试、兼容拒绝 reasoning_effort/logprobs；视觉与文字模型区分 | `backend/app/services/ai_service.py:232–289,422–529,683–806` |

## 领域模型与技术架构

1. `User` 是身份主体，带时区、语言、位置、身体尺寸与家庭归属（`backend/app/models/user.py:25–82`）；`Family`/`FamilyInvite` 管理成员邀请（`backend/app/models/family.py:15–60`）。本地软件需要额外区分“无需登录的衣橱主人/家庭档案”与“协作者身份/设备”。
2. `ClothingItem` 含图片/缩略图/原图、分类、AI 任务生命周期、穿着/推荐/接受计数、洗护、购买信息、收藏、归档（`backend/app/models/item.py:46–160`）；`ItemHistory`、`WashHistory`、`ItemImage` 为事件及附件实体（同文件:163–219）。
3. `Outfit` 包含天气快照、场合、预定日期、理由、风格建议、人工属性、来源、状态、替代/克隆链；`OutfitItem` 保存顺序/层次；`UserFeedback` 区分满意与实际穿着，家人评价独立（`backend/app/models/outfit.py:45–209`）。
4. `UserPreference` 是显式偏好；`UserLearningProfile`、`ItemPairScore`、`OutfitPerformance`、`StyleInsight` 是学习派生数据（`backend/app/models/preference.py:24–55`；`backend/app/models/learning.py:80–280`）。这两个层次应保持分离，允许用户修正/重置模型推断。
5. 运行栈为 Next.js 14.2.35/React 18/TypeScript、TanStack Query、Tailwind/Radix、next-intl/NextAuth；Python FastAPI/Pydantic/SQLAlchemy async、asyncpg/Alembic、Pillow、Redis/arq、SMTP（`frontend/package.json:2–94`；`backend/requirements.txt:1–56`）。Docker 编排 PostgreSQL 15、Redis 7、API、前端、worker（`docker-compose.yml:8–9,41–140`）。

## 应吸收的优点

- 吸收完整业务闭环：录入 → 核对标签 → 推荐/手工搭配 → 选择 → 实际穿着 → 洗护 → 反馈 → 学习 → 衣橱分析，而不是仅生成图片。
- 吸收“规则先筛候选、模型再组合、代码校验结果”的推荐设计，并保留评分因子与理由，便于解释和离线规则降级。
- 吸收建议、Lookbook 模板、实际穿着记录的语义区分，避免点击“喜欢”就自动记一次穿着。反馈应保留“改穿了什么”。
- 吸收队列幂等、持久进度、可取消/可重试、模型来源/原始结果保留、原图恢复、多尺寸图片，减少录入丢失和生成失控。
- 吸收显式偏好与学习画像分层，以及手工操作/真实穿着对配对学习的贡献；所有派生统计应可从事实事件重算。
- 吸收家庭隔离、时区与提前一晚推荐、渠道 provider 抽象、外部 Agent 写回与能力发现。

## 不能直接继承的承诺与待改设计

- **不是本地 SQLite 架构**：模型使用 PostgreSQL UUID/JSONB/ARRAY，数据库和 worker 有服务器依赖（`backend/app/models/item.py:49–78`；`docker-compose.yml:8–9,41–140`）。桌面版应抽取领域规则重新适配 SQLite/本地任务调度，不应隐藏 Docker 当作“单机安装”。
- **离线 UI 不等于本地优先同步**：IndexedDB 主要保存未上传图片任务（`frontend/lib/upload-queue.ts:3–45`）；没有发现实体变更日志、WebDAV 双向同步、冲突合并、E2EE 或可独立运行 CLI/MCP 实现。README 的 works offline 出现在 Ollama 模型配置语境（README.md:210），天气/地理编码仍会访问网络（`backend/app/services/weather_service.py:142–151,309–314`）。
- **图片处理不是生成/试穿**：图像 service、去背景 provider 和图片 worker 的核心是上传变体/去背景，不应据此声称已经有 AI 商品展示图、穿搭合成或虚拟试穿。
- **学习的实际触发需要精确复用**：`accept`/`reject` 路由主要写状态（`backend/app/api/outfits.py:790–834`），完整反馈才调用 process_feedback（同文件:968–969）；新系统应统一领域事件触发，避免多个入口行为不一致。
- **统计基线有限**：存在购买价格字段（`backend/app/models/item.py:132–133`），当前 analytics 响应没有成本/单次穿着成本/购买决策模块（`backend/app/api/analytics.py:57–76`）。周/月统计用 UTC 和滚动 7/30 天（同文件:108–110），新系统需要清楚标注并遵循用户时区。
- **同步不能直接同步计数**：现有穿着/洗护直接更新计数（`backend/app/services/studio_service.py:104–123`），跨设备应以不可变事件 ID 去重后重算，否则多个设备可能丢增量。
- **删除需要重新设计**：当前 delete 先删除图片再删除单品（`backend/app/api/items.py:1287–1312`）；本地版应有事务性 tombstone、回收站与可恢复附件，保证同步与恢复安全。
- **“任何 OpenAI 兼容模型”是接口意图**：有兼容性 fallback 代码，但各模型的视觉/JSON/限流能力仍需 capability 注册和实际验证，不能等同于所有供应商均验证。
- **定时降级明确**：内部文字 AI 关闭时定时任务直接 skipped（`backend/app/workers/notifications.py:244–247`）；外部 Agent 模式要自行补齐定时 orchestrator。若电脑休眠/关机，桌面任务要有补执行策略。

## 许可证与验证范围

根 `LICENSE:1–21` 为 MIT，Copyright (c) 2026 Anish S.：可复用/修改/分发，但复制代码或实质部分需保留版权及许可。许可证不替代外部模型、图片、依赖或服务条款的核查。

仓库含 83 个 backend/frontend 测试目录文件（文件计数，并非 83 个测试用例），涵盖 recommendation、analytics、上传队列、家庭成员选择等。此次没有运行它们；后续实现应验证跨入口一致性、实际穿着不重复记账、附件删除恢复、任务断点恢复、家庭访问控制以及时区/休眠调度，不能把“有测试文件”写成“测试通过”。
