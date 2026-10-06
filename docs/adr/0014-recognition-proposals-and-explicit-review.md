# ADR0014：视觉建议独立存储，人工审核才写事实

- 状态：接受
- 关联：GitHub #13，F05/F30；ADR0008 持久任务、ADR0013 商品图 adapter。

## 问题

视觉模型会误判衣物、猜测品牌，成功连接或结构化 JSON 合法不代表事实正确。原图同步限制也不等于用户拒绝本次明确模型上传。重分析必须保护人工更正，批量一项模型失败不应阻断手工入库。

## 决策

DB12 新建 `recognition_suggestions`（job_id 唯一、member_id 隔离）及成员索引。所有新旧版本初始化修复实际表后设置 max(user_version,12)，与独立 DB10/11 迁移合并。识别采用已有 OpenAI-compatible vision adapter；provider/model/endpoint、目标、选定 asset IDs、原图或展示图角色、来源哈希、基线快照在任务创建时冻结。上传须显式确认；最多20张、所选源合计20MiB。原图同步仍 local_only。本次只将选定版本按 EXIF 朝向转换为最长边512px PNG，传输图片哈希、尺寸和转换名写来源；不把本机路径、密钥或 base64 网络请求写入识别任务/建议。

模型原始视觉描述与来源、用量、时刻、prompt版本保存为未审核建议。保守通用词汇规则只能将描述中的衣物词/颜色规范化为候选，歧义/否定时弃权；UI 明示这是词汇候选。不得由 fixture 名称、预期结果或模型自报分数生成事实。模型原话中的不确定项原样保留，固定提示说明品牌/材质/尺寸无法确认；自报分数不视为准确率。

审核输入须显式提供经核实的字段并确认；沿用 clothing canonical taxonomy 校验。单品或草稿当前完整基线与创建时不符则 `review_conflict`，避免重分析覆盖其后的人工改动。单品事实、audit/outbox与建议 reviewed 状态同一 SQLite 事务写入；草稿事实与 reviewed 同一事务，不提前入库。手工 clothing/imports 保存完全独立。

POST 前持久化 submitted 并校验 lease。超时、截断响应、5xx或提交后重启均等待核实，submitted 任务取消后也禁止 jobs.retry 盲重发。429 已明确拒绝可按 Retry-After（1–300秒）继续同一任务，最多三次尝试；401/参数/模型拒绝不自动重试。所有结果发布与429重新排队再次校验 lease，取消不被响应复活。若用户核实后需要新分析，须再次确认供应商/图片并创建新操作意图。

## 验证与局限

真实 SmolVLM256M 的 JSON分类曾将白衣识为鞋；WebP输入曾返回400；改为规范化PNG后黑白鞋和蓝牛仔裤描述实用，白衣仍猜测品牌。全部失败/原话保留，人工路径和来源提示为必须流程。小模型质量不宣称生产准确率；真实本地调用、外部HTTP边界fixture、Chrome真实Application bridge、原生Tauri未实测各自分开记录。
