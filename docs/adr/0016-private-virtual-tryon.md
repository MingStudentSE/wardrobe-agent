# ADR 0016 — 本机私有虚拟试穿与多方案对比

2026-10-07 · accepted · ticket #21 / F16

## 决策

采用 Ark Seedream 5.0 Flash 多图编辑适配器，要求 provider 同时显式声明 `image_generation` 与 `virtual_tryon`。官方 [图片生成 API](https://docs.volcengine.com/docs/ark/image-generation-api?lang=zh) 支持 2–10 张参考图，单张边长 >14、宽高比 1/16–16、最多30MB/36MP；[官方提示词指南](https://www.volcengine.com/docs/82379/1829186) 展示人物穿参考服装的编辑语义。选择该通路是基于多图编辑能力的实现决策，不宣称它是物理合身预测或已完成真实云效果验收。

人物固定为第一张，随后完整发送所选不可变搭配版本的每件展示图，最多9件衣物。逐件核验缺图、尺寸与哈希；超限整次拒绝，绝不截断。提示词明确全部衣物、人物身份/姿态、衣物角色层次、颜色/版型/纹理保真。复用 Ark 的实际 `/models` 开通检查和 `/images/generations` 调用；Flash 不传组图参数。

人物导入要求本人/模特来源与使用权确认，解码应用EXIF旋转，重编码PNG去元数据。使用 `tryon_person` 本机 jobs 记录和专用 CAS 文件夹；不创建可采用的 clothing asset，不写 outbox。试穿结果同样仅本机 job + WebP CAS。无新表/迁移；不占DB14。明确保存只把本机任务置成功，不改衣物、搭配、穿着或同步清单。

每次任务冻结 person hash、outfit id/version、每件clothing/asset/hash/role/layer和provider/model。后台共享持久化租约、提交前检查取消、提交时间先持久化、迟到结果不复活、未知结果不自动重发。失效前置条件可修复后重试；已提交/未知只能查供应商记录，用户明确新意图才可另建计费任务。

`tryon.compare` 最多4个不同方案，每个返回自己的前图与结果/来源，不混用不同人物的前图。GUI、CLI、MCP共享 Application 授权、成员隔离与同一错误语义。页面沿用Atelier，窄屏顺序前后图，宽屏并排；不用拖拽/hover，先核对人物再上传。

## 验收边界

协议fixture使用原创虚构人形图与合成结果验证全部图片传输、取消/未知、对比/私有保存，不作为真实试穿效果。真实已开通付费模型、有人像使用权的输入、全衣物保真、人体一致性和多件语义效果尚待真实云验收；无用户key/照片时保持门槛，不关闭 #21。不提供尺码、身体尺寸或合身承诺。
