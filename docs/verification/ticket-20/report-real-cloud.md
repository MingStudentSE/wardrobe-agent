# #20 F15 AI 搭配图（flat-lay）——真实云供应商验收报告（real-cloud）

2026-10-08 执行，环境同 [ticket-14 real-cloud 报告](../ticket-14/report-real-cloud.md)（同一 CLI、同一隔离数据目录、同一 provider `01a118cf-add6-778a-a93a-46d9ab21a8f8`，adapter `ark_seedream`，model `doubao-seedream-5-0-flash-260915`，凭据经 stdin 钥匙串配置、日志无密钥）。

## 前置

- 复用 #14 验收后的三件单品与当前展示图：tee（生成并已采用的 AI 商品图，display hash `8b80502e…`）、coat（夹具压缩图 `116398ec…`）、shoes（夹具压缩图 `b49bad81…`）。
- `outfit.create` 保存三件搭配：outfit `01a118d3-4ad6-71f5-8ebe-6787b617627f` version 1（items: top layer 0 / outerwear layer 1 / shoes layer 0）。

## 真实生成（调用 3/6）

- `flatlay.generate`（`upload_images: true`，operation_key `real-acceptance-flatlay-1`，version 1）→ 任务收据 job `01a118d3-63a9-749f-b9a2-0577a5308791`（kind `outfit_flatlay`）。
- **任务收据明确上传全部展示图**：payload.inputs 恰为保存版本 v1 的 3 件展示图，逐件记录 `asset_id` + `display_hash`（`8b80502e…` / `116398ec…` / `b49bad81…`）与 role/layer；provider 段记录 endpoint 与 model。
- `flatlay.process` 推进，真实多参考图提交耗时 **25s** → `waiting_input`，attempts 1，`request_id 02179141788176848e426867c3c446aaa68159641220ac14ac3b0`。
- 结果：blob `56591dab6761bd15dd87780e88b07567e8d82cbba4156dd46ea306c8d123f56d`，100 598B，高 1600，`adopted: false`（等待明确保存）。

## 预览、保存与断言

- `flatlay.preview` 返回本机 webp data_url；解码存证 SHA256 = `56591dab…f56d`，**与任务收据 blob_hash 完全一致**（下载/预览完整性闭环）。
- 人工质量核对：三件单品真实合成 flat-lay 构图（白/米色背景上平铺黑色印花 T 恤、白黑拼色"GENIUS FORM"外套、黑高帮鞋），T 恤印花与 #14 生成商品图一致，无文字畸变干扰主体识别。
- `flatlay.save` → succeeded，`adopted: true`（保存进搭配版本，本机目录）。
- **不产生穿着断言**：生成前、flat-lay 采用后、#21 试穿后三次 `wear.list` 均为空（`items: []`，日志 30/37）；输出 `sync_scope: local_only`，结果仅存在于隔离数据目录（备份盘），未进入任何同步 manifest。
- 保留语义：失败/未保存不改变已保存搭配或穿着——本流程 save 前任务处于 `waiting_input`、outfit v1 与 wear 全程未变，与协议一致。

## 存证

| 文件 | SHA256 | 说明 |
| --- | --- | --- |
| `results/continue-20261007/real-ai-acceptance/evidence/flatlay-generated-1.webp` | `56591dab…123f56d` | 三件搭配的真实 flat-lay 生成图（已 save） |

## 验收项逐条核对

1. 真实付费多参考图（全部展示图）调用成功 —— 满足（1 次生成、3 输入图带哈希收据）。
2. 保存版本全量来源追溯 —— 满足（任务 payload inputs/outfit_version/provider + result.request_id/blob_hash）。
3. 仅本机、不产生穿着 —— 满足（wear.list 空、local_only）。
4. flatlay.save 本机保存 —— 满足（blob 哈希与存证一致）。

F15 的"真实云生成成功"缺口由本报告闭合。
