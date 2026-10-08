# #14 F07 商品展示图——真实云供应商验收报告（real-cloud）

2026-10-08 执行。集成 tip `a84c8c6` 的 debug CLI（wardrobe-cli 0.1.0，`/Users/links/.codex/wardrobe-execution/worktrees/integration-continue/target/debug/wardrobe-cli`）。隔离数据目录 `/Users/links/.codex/external-workspace-volume/codex/build-temp/real-ai-acceptance-data`（备份盘，全新，未触碰用户真实数据目录）。原始命令日志：`results/continue-20261007/real-ai-acceptance/logs/`，生成图存证：同目录 `evidence/`。凭据经 CLI 钥匙串（stdin）配置，本报告与全部日志不含密钥（已对日志做密钥值全量扫描：0 命中）。

## 供应商配置

| 项 | 值 |
| --- | --- |
| provider id | `01a118cf-add6-778a-a93a-46d9ab21a8f8` |
| name / adapter | 真实验收-ArkSeedream / `ark_seedream` |
| endpoint | `https://ark.cn-beijing.volces.com/api/v3` |
| model | `doubao-seedream-5-0-flash-260915` |
| capabilities | `image_generation`, `virtual_tryon` |
| authentication | bearer；`provider.credential.set` 经 stdin 写入钥匙串，`provider.credential.status` 返回 `configured: true`，`provider.list` 读回配置且无任何密钥字段 |

`provider.validate`（text 与 vision 两种能力）均返回 `unsupported_capability`（exit 2）：该 adapter 仅配置图像能力，注册面语义即"不允许 text 假验证、validate 不覆盖图像生成"（与 #14 实施报告一致）。因此图像生成能力的真实验证 = 下述真实生成成功。

## 测试夹具与基线

- `clothing.create` 三件：真实验收-白色T恤（top `01a118ce-e176`）、真实验收-深色外套（outerwear `01a118ce-e1a3`）、真实验收-运动鞋（shoes `01a118ce-e1b1`）。
- `assets.import` 导入 `design/studio/assets/` 真实夹具照片（tee/coat/shoes.png），压缩管线确认：display webp quality 82 role product_display（tee 750×1000/14 064B，id `b502ab6a…`），`assets.adopt` 后 `display_asset_id` 生效（日志 04/05）。

## 第一次真实生成（调用 1/6）

- `product.generate`（`upload_original: true`，operation_key `real-acceptance-product-tee-1`，asset `01a118ce-ff8a…` 原图 122 726B）→ 任务收据 job `01a118d0-4b09-77da-9707-a990008bdc21`（kind `product_display`，state queued，payload 含 provider 配置与 base_display_asset_id）。
- `product.process` 推进，真实提交耗时 **21s** → `waiting_input`（等待人工采用），attempts 1，`request_id 0217914177119423f04613060c95c0e1cb70b4f628471a948bd5f`，`verified_capabilities: ["image_generation"]`。
- **采用前核验**：`clothing.get` 显示旧展示图 `01a118ce-ff8a…` 不变（生成不改变旧图）。

## 人工质量核对与采用

生成图（存证 `evidence/tee-generated-1.webp`，1200×1600 webp 31 038B，SHA256 `8b80502e5f533f4cc53da26671d989451f1df74516acb08ea6b1fb41ed61d94b`，与资产 display id 一致）：白色背景下黑色印花 T 恤平铺电商图，衣型对称、印花（绿紫竖条+徽章+文字）位置合理、无畸变。人工核对通过后：

- `product.adopt` → succeeded；`clothing.get` 确认 `display_asset_id` 变为生成资产 `01a118d1-48a5-7101-a949-76bde4ab4615`（00:02:45Z）。
- 采用后展示图即压缩 webp（31 038B，quality 82，product_display role），来源追溯字段完整：资产 `lineage[0]` 记录 `action: product_generation`、`algorithm: wardrobe-image-1`、`parameters: {adapter: ark_seedream, model: doubao-seedream-5-0-flash-260915, provider_id, request_id, job_id}`、`source_display_id`（导入压缩图）与 `source_original_id`（本机原图）。

## 显式重生成且不采用（调用 2/6）

- 第一次请求误用已被替换的旧资产 ID，被本地校验拒绝（`invalid_input: 请选择此单品的现有图片`），未提交供应商、未消耗额度——采用后单品资产列表仅含当前展示图。
- `product.generate`（operation_key `real-acceptance-product-tee-2-regen`，asset=当前展示图 `01a118d1-48a5…`）→ job `01a118d2-5bd3-7560-93e1-26310dad2f65`；`product.process` 21s → `waiting_input`，`request_id 0217914178067915e054c6d0ba743fab13ed8d72af4b8b2e48528`。
- **不采用**：新结果仅为 preview 资产 `01a118d2-bc8a…`（display `e5343b64…` webp 1200×1600 32 354B）。核验 `clothing.get` 展示图仍为 `01a118d1-48a5…`，当前展示文件 SHA256 仍为 `8b80502e…` 不变。lineage 形成两代链（gen1 结果作为 gen2 来源），每代均带 provider/model/request_id/来源图。
- 结论：**"旧图保持直到显式采用；失败/不采用保留旧图"语义在真实供应商语境下成立**（成功生成的新图不自动上屏）。

## 存证

| 文件 | SHA256 | 说明 |
| --- | --- | --- |
| `results/continue-20261007/real-ai-acceptance/evidence/tee-generated-1.webp` | `8b80502e…61d94b` | 第 1 次生成，已采用为展示图 |
| `results/continue-20261007/real-ai-acceptance/evidence/tee-generated-2-not-adopted.webp` | `e5343b64…40c66e6` | 第 2 次重生成，未采用，仅 preview |

## 真实调用验收项逐条核对（对照"必须保留的未完成验收"）

1. 真实供应商付费图像调用成功 —— 满足（2 次 Seedream 5.0 Flash 真实生成，均返回 request_id 与图像）。
2. 在明确上传许可（upload_original=true 显式参数）下仅上传所选单品原图 —— 满足（任务收据 payload 记录 upload_original 与资产/原图 ID）。
3. 旧图不变 → 预览 → 显式采用 → 展示图替换且压缩 —— 满足（08:02–08:03 本地时间全链路）。
4. 来源追溯（provider/model/输入图）—— 满足（lineage 与任务 payload 双记录）。
5. 重生成为新的明确意图、不采用不改旧图 —— 满足。
6. 实际衣物结果质量人工核对 —— 已执行（色彩/版型/印花与夹具一致、构图完整）；此为验收人本次 single-pass 核对，后续衣物级质量仍以用户实际使用为准。

#14 的"真实云生成"缺口由本报告闭合；#14 保持 OPEN 的其余理由（如有）不在本报告范围。
