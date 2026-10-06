# AIWardrobe 源码审计

审计日期：2026-10-06。仓库：https://github.com/leoz9/AIWardrobe 。固定 commit：`9a25eca94faa4ebe815a9660eeb94e5513532319`。方法：静态检查实际路由、服务、数据模型和页面；未安装依赖、未启动、未调用外部模型，因此“已实现”表示源码存在，不表示运行效果验证通过。项目及本目录未发现适用 AGENTS.md。

## 定位和实际结构

这是 React 19 / Vite 浏览器前端 + Python FastAPI 后端 + SQLite 的个人智能衣柜。后端可托管前端构建产物，挂载上传图片（`backend/main.py:43-62,93-126`），不是独立桌面安装包。数据库使用 aiosqlite，路径可由 DB_FILE_PATH 配置（`backend/storage/db.py:26-35`）。

## 功能事实与吸收价值

| 能力 | 实现证据 | 值得吸收 | 边界 |
|---|---|---|---|
| 拍照/图片录入 | `backend/api/upload.py:24-100`；`frontend/src/components/Upload.jsx` | 上传→抠图→视觉识别→入库的短流程 | 后端单次一张；没有多件服饰区域分割 |
| 批量录入 | `frontend/src/contexts/UploadContext.jsx:53-121` | 单张失败不中断整批，成功/失败计数，全局上传状态 | 浏览器逐张串行；没有可恢复的后台任务队列，重启丢状态 |
| 本地/远程抠图 | `backend/api/upload.py:47-61`；`backend/services/segment.py:25-51` | rembg 本地隐私路径；remove.bg 失败回退本地 | 本地依赖未安装会失败，模型首次运行是否需下载未运行验证 |
| 语义识别 | `backend/services/openai_compatible.py:89-158` | OpenAI 兼容 Vision，自定义模型/URL，Pydantic 校验输出 | 没有 key 会中断录入；识别发的是抠图结果，没有置信度/模型版本存档 |
| 人工修正 | `frontend/src/pages/Entry.jsx:39-75`；`backend/api/wardrobe.py:79-85` | 自动识别后可编辑，避免模型结果直接成为唯一事实 | 上传 API 已先入库，编辑不是待审核事务 |
| 衣柜管理 | `backend/api/wardrobe.py:19-94`；`frontend/src/pages/Wardrobe.jsx:73-101` | 分类、搜索、季节/风格筛选、详情编辑 | 只有 top/bottom/shoes/accessory 四大类；没有家庭成员、购买成本、洗涤/穿着历史 |
| 手动穿搭 | `frontend/src/pages/Outfit.jsx:8-12,63-127` | 上下装/鞋轮播、季节筛选、随机组合，视觉反馈直接 | 搭配只在页面 state，数据库没有 outfits 表；没有保存搭配、穿着日历 |
| 天气场景推荐 | `backend/services/recommendation.py:373-545` | 硬约束、评分理由、衣柜已有与补购明确区分，三种权重模式 | 规则挑选单品，再让 LLM 写文案；不是 LLM 全局搭配求解 |
| 星座/幸运色 | `backend/storage/models.py:28-48`；`backend/services/recommendation.py:238-250,400-404` | 可选的轻趣味个人化入口 | 星座不应作为必要推荐证据；最终项目建议做可关闭插件 |
| 自定义试穿接口 | `backend/services/tryon.py:31-81`；`backend/api/tryon.py:19-68` | person+garment 的 Provider 适配，接受图片字节/URL/base64 | 默认 disabled，没有内置试穿模型；详情页支持本人照片与单件试穿（frontend/src/pages/ClothesDetail.jsx:72-100,188-217）；只传一件服饰，返回 URL 不落本地，服务效果未验证 |
| 多语言/主题 | `frontend/src/i18n/locales/{zh,en,ja}.json`；`frontend/src/contexts/ThemeContext.jsx` | 中文/英文/日文与主题切换 | 未做 UI 运行核验 |

前端页面路由只有首页、录入、衣柜、详情、手动搭配、推荐（`frontend/src/App.jsx:16-23`）。试穿入口位于衣物详情页而非独立页面：上传本人照片、调用 /tryon、展示结果（`frontend/src/pages/ClothesDetail.jsx:72-100,188-217`）。因此前后端连接代码均已实现，但不能把它称为已验证的模型效果。

## 推荐机制：规则选衣，LLM 讲解

1. 获取当前衣物与天气/星座，归一化目标与温度策略（`backend/services/recommendation.py:383-406`）。
2. 以季节标签作温度兼容硬筛选，分别产生上装、下装、鞋候选（`:408-422`）。
3. 每类独立打分，分数包含季节、幸运色、风格、雨天鞋需求、场景，`pick_best_item` 取最高分（`:207-299`）；同分保留先遇到的衣物。数据库按创建时间倒序返回（`backend/storage/db.py:83`），因此重复生成可能偏向同一批新衣。
4. 找不到合温度的上/下装则建议补购；鞋可从全部鞋中兜底，不满足温度并非始终严格淘汰（`backend/services/recommendation.py:429-463`）。配饰取评分最高两件，缺失时给购买配饰建议（`:468-502`）。
5. LLM 接收已经选好的衣服与理由，生成推荐文案；没 key 或调用失败，回退规则文案（`:504-514,548-574,659-681`）。它没有重新返回衣物 ID，亦未见全套颜色相容性、剪裁比例、层次、裙装/连体类别的组合优化。

这条“确定性选择 + 可解释文案 + 离线规则兜底”值得保留，但最终 Agent 需要组合级评分、用户反馈/穿着历史、硬约束验证及多候选。没 key 时推荐可兜底，不等于整个录入离线可用。

## 数据模型与图片生命周期

衣物表只有自增整数 id、category/item、三类 JSON 语义、color/description、image_filename 和 created_at（`backend/storage/models.py:7-19`）。此外有星座记录、天气缓存、地点解析缓存（`:28-85`）；没有衣物修改时间/版本/软删除/设备 ID/变更日志，不能直接用于多设备同步。

**原图没有保留。** `raw_bytes` 读取后抠图并用于分析，最终只把 `processed_bytes` 保存为随机 PNG，数据库只引用这一张图片（`backend/api/upload.py:41-71,81-90`）。识别失败发生在保存前，所以失败上传也没有原图档案。最终设计必须先存原图，再存抠图/商品展示图/试穿等派生资源，并记录来源、变换参数和任务版本。

衣物删除直接 `DELETE FROM clothes`（`backend/storage/db.py:118-126`），不是回收站，未见对应图片清理；前端衣柜卡片有删除与浏览器 confirm（`frontend/src/pages/Wardrobe.jsx:49-50,167-171`）。最终设计应改成详情中的软删除、撤销/回收站，以及安全的资源垃圾回收。

## 密钥、安全与部署限制

**API key 明文落盘。** `backend/storage/config_store.py:10,44-49` 把包含 LLM、remove.bg、try-on key 的完整 Pydantic 配置写入 `storage/llm_config.json`（字段见 `backend/domain/config.py:24-36`）。`get_masked_config` 仅在返回前遮挡 key（`backend/storage/config_store.py:108-139`），不是磁盘加密或系统密钥链。

FastAPI 路由注册中未见鉴权依赖；CORS 允许所有 origin、method/header，并启用 credentials（`backend/main.py:43-62`）。另有 `/api/install-rembg` 服务端执行当前 Python 的 pip install（`backend/api/config.py:57-96`）。若改成桌面应用，应以安装包附带或受控下载模型取代网页触发安装；本地 IPC/HTTP 应认证且限制接口，密钥写系统 Keychain/Credential Manager/Secret Service，默认不进入同步包。

无 CLI、MCP、桌面壳、同步引擎、E2EE、移动原生客户端或长期任务服务的源码证据；不能因本地 SQLite 就称为已具备 local-first 多端架构。

## 许可证与复用结论

根 LICENSE 为 MIT，Copyright (c) 2024 leoz9（`LICENSE:1-21`）。复用代码需要保留版权与许可；第三方模型权重、SDK、remove.bg 服务和实际云端模型条款应分别检查，根 MIT 不覆盖这些服务/模型。

最适合吸收的是：低摩擦语义录入、人可纠错、本地抠图与云端替代、规则候选的解释性和失败兜底、批量失败隔离、轻量手动搭配 UI。应重设计的是：原图与派生媒体、安全凭据、组合推荐、完整衣橱领域模型、持久任务、共享核心与多端同步。它适合作为交互/服务流程参考，不适合直接承担最终系统的数据库和同步核心。
