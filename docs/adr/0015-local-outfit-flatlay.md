# ADR 0015：保存版本的本机 AI 搭配图

状态：接受实现；真实云生成效果验收待用户安全配置后执行。

F15 使用已保存的不可变搭配版本，上传该版本全部商品展示图。单件搭配仍可生成；多件必须逐件具备可解码、哈希匹配的图片。缺图、数量或分辨率不符合供应商能力时提交前失败，不静默截断，不改变搭配或穿着事实。本地拼贴继续可用。

复用专用 Ark Seedream 图片适配器、操作系统凭据和现有 jobs 表。依据官方 [图片生成 API](https://docs.volcengine.com/docs/ark/image-generation-api?lang=zh) 与 [Seedream 模型说明](https://docs.volcengine.com/docs/ark/seedream-4-0-5-0?lang=zh)，当前已实现协议限定 Seedream 5.0 Flash：1–10 张参考图；单图字符串、多图数组；每图尺寸大于14像素，长宽比 1/16–16，最多3600万像素及30MB。先查询已启用模型，条件发送 size=2K，不发送该模型不支持的 sequential_image_generation。其他模型及试穿能力必须单独核对协议。

提交标记在 HTTP POST 前持久化。专用同步接口没有已确认的结果查询协议，因此提交后断连、超时或崩溃恢复进入 waiting_input；禁止同任务重发，重新生成必须明确创建新意图。取消会使 lease 失效，迟到结果不能回写。

生成任务保存 outfit_id/version 和每项 clothing_id、display_asset_id、hash、role、layer。结果先验证解码和尺寸，再压缩为 WebP；内容寻址文件放入独立 outfit-generated-local-only 目录，仅任务引用，不登记同步资产。查看时再次校验哈希。明确采用只结束本机任务，既不改写模板，也不产生 wear 或同步 outbox。无需新增数据库表或迁移。

协议 fixture 证明真实 HTTP/持久化/客户端链路，不证明付费云生成质量。最终云验收需至少两件真实衣物、账户已启用模型、实际请求和人工检查全部衣物及颜色版型。
