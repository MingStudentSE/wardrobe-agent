# #21 F16 虚拟试穿——真实云供应商验收报告（real-cloud）

2026-10-08 执行，环境同 [ticket-14 real-cloud 报告](../ticket-14/report-real-cloud.md)（同一 CLI `a84c8c6`、同一隔离数据目录、同一 provider `01a118cf-add6-778a-a93a-46d9ab21a8f8`，adapter `ark_seedream`，model `doubao-seedream-5-0-flash-260915`，日志全量扫描无密钥）。

## 合成模特图（直接 ARK 调用，调用 4–5/6）

CLI 注册面对图像生成仅开放衣物锚定流程（product/flatlay/tryon），无自由文生图操作；按验收计划用**同一 provider/model** 直接调用 ARK `POST /api/v3/images/generations` 生成人物素材。密钥仅在进程内从 0600 密钥文件读入 HTTP 头，不经 argv、不入日志。

- 参数：model `doubao-seedream-5-0-flash-260915`，prompt「全身站姿年轻女性模特，素浅灰色纯色背景……白色圆领T恤与深蓝色直筒牛仔裤……」（自拟），size `1024x1024`，`response_format: b64_json`，`watermark: false`。
- 第 1 次（10s）生成成功，但验收脚本把 b64 脱敏后才落盘、图片字节未保留（执行侧失误，额度已消耗、已如实记录）；随即同参数重拍（9s）成功：JPEG 1024×1024，137 313B，`usage.generated_images: 1`。
- 该图为 **AI 合成人像，不含任何真实人物**，无真人肖像/权利问题；`rights_confirmed: true` 的含义即对图内人物权利的确认（合成、自持、无第三方）。

存证：`evidence/model-photo.jpg`，SHA256 `e111bbd9de8b74b6eaaf5f3ad126bc69e93e4c9d352dd81db76515915454b4e6`。人工核对：全身、正面、直立站姿、素背景，符合 prompt。

## 人物导入

- `tryon.person.import`（`person_kind: "model"`，`rights_confirmed: true`，operation_key `real-acceptance-person-model-1`）→ job `01a118d6-1e5e-714c-ac4d-ced369cbd4c7`（kind `tryon_person`），进度即满，`sync_scope: local_only`，人物 blob `ef1b3668f0ce687960d0d0bb10e6451b8f0c8901f0a224ec88aeb16d9fc358d7`，仅存于数据目录 tryon-person-local-only，不进入衣物或同步。

## 真实试穿（调用 6/6）

- `tryon.generate`（人物 + outfit `01a118d3-4ad6…` v1 全部衣物，`upload_images: true`，operation_key `real-acceptance-tryon-1`）→ 任务收据 job `01a118d6-4394-7359-8786-8ba6fe8d3d67`（kind `virtual_tryon`）。
- **收据输入清单**：person（blob `ef1b3668…`，与导入哈希一致）+ inputs 三件展示图（tee `01a118d1-48a5…` hash `8b80502e…`、coat `01a118cf-2d56…` hash `116398ec…`、shoes `01a118cf-305c…` hash `b49bad81…`）。
- `tryon.process` 推进，真实提交耗时 **24s** → `waiting_input`，attempts 1，`request_id 021791418071080add9fca5277b8939d33af9a0f1789d6c43dea7`。
- 结果 blob `50f24c9dc366b08b1406fba7b52931ddc4279c6bd3d25acd5755bc62c3f6f95b`，82 814B，高 1600，`adopted: false`。

## 对比图、保存与断言

- `tryon.compare`（ids=[上述 job]）：返回 before/after 与 `fit_guarantee: false`（明确不预测尺码/合身）。before（PNG）SHA256 = `ef1b3668…` = 人物导入 blob（原始人物照未改动）；after（webp）SHA256 = `50f24c9d…` = 试穿结果 blob——对比图两栏与收据逐字节一致。
- 人工核对 after：同一人物（脸型/发型/姿态/牛仔裤保留）穿上三件单品——黑色印花 T 恤（与商品图印花一致）、白黑拼色外套（GENIUS FORM 袍袖条纹可辨）、黑色高帮鞋（绿/紫鞋带与夹具一致）；构图完整、无明显肢体畸变。合身度不做断言（产品语义即不预测）。
- `tryon.save` → succeeded，`adopted: true`，blob `50f24c9d…` 本机保存。
- **不产生穿着断言**：试穿保存后 `wear.list` 仍为 `items: []`（日志 37）；`sync_scope: local_only`，全部产物仅存隔离数据目录。

## 存证

| 文件 | SHA256 | 说明 |
| --- | --- | --- |
| `evidence/model-photo.jpg` | `e111bbd9…54b4e6` | AI 合成模特全身照（试穿人物输入） |
| `evidence/tryon-compare-before.png` | `ef1b3668…9fc358d7` | compare before = 人物原图 |
| `evidence/tryon-compare-after.webp`（=`tryon-result-1.webp`） | `50f24c9d…c3f6f95b` | 真实试穿结果（已 save） |

## 验收项逐条核对（对照 #21 报告"必须的真实验收"）

1. 真实已开通云账户 + paid call —— 满足（doubao-seedream-5-0-flash-260915 真实计费调用返回 request_id 与图像）。
2. 明确使用权人物图 —— 满足（AI 合成模特、rights_confirmed=true、person_kind=model，无真人权利问题，已说明）。
3. 全部衣物输入 —— 满足（收据三件展示图带哈希）。
4. 实际语义/保真效果 —— 人工核对通过（三件衣物均出现在结果中、人物身份保持、非套装下装保留）；loopback 协议测试之外的真实推理链路打通。
5. 不预测合身 —— 满足（fit_guarantee=false，产品未做尺码断言）。
6. 仅本机、不产生穿着 —— 满足（wear.list 空、local_only）。

F16 的"真实云试穿"缺口由本报告闭合；原生 GUI 文件选择器等平台验收仍按原票范围另行追踪。
