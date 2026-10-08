# ADR 0013：专用商品图生成、未知结果与采用

状态：实施中；#14 / F07，实际云账户生成质量验收待用户安全配置供应商。

`ark_seedream` 与 `openai_compatible` 是不同 adapter。专用配置只声明 image_generation，不用文本验证证明图片能力。第一版真正实现 Seedream 5.0 Flash 的单张参考图→单张商品图：模型 ID 和最终 HTTPS 端点由用户配置，本机 HTTP 仅供测试/本地端点。先 GET models 检查供应商返回的模型 ID，再 POST images/generations。凭据沿用 #12 OS 安全存储和 endpoint fingerprint，任何 key 不进入数据库、命令行、审计或同步。

每次明确原图上传授权形成 member+kind+operation_key 幂等 jobs。网络不持写锁；提交前短事务记 submitted_at，取消使 lease 失效，响应与 preview Asset 保存同一事务，迟到结果不可回写。原图校验SHA、大小/比例/像素限制，方向规范化成内存 PNG 去除元数据后发送；仅选定的一张原图，不自动发送衣橱或另一成员照片。

Ark 图片生成接口为同步请求，无已文档化的图片任务 query/cancel API。不得借用视频/3D任务 API 查询图片任务。失联、超时、崩溃和供应商错误保留结果未知 waiting_input，记录不含密钥的错误；已提交 job 永远不能被通用 jobs.retry 或 lease 接管再次发送。用户可检查供应商记录并明确创建新的生成意图；GUI标记这可能再次使用额度，不能承诺退款或撤回供应商调用。未提交的明确失败才允许同任务重试。关闭界面后不启动隐藏网络 worker，重开显示持久状态并允许继续 queued 任务。

生成返回 base64 图片经过有界读取和实际解码，然后复用统一1600最长边、q82 WebP、缩略图及 CAS 流程。新的 preview Asset 沿用本地 source original 引用，lineage 记录 adapter/provider/model/job/request；不覆盖旧资产，不修改衣物事实。预览仅本机。product.adopt 原子更新衣物主图、采用状态、job成功、audit/outbox，要求衣物未删除且原有主图仍等于提交快照，避免迟到生成覆盖新编辑。已有旧展示资产和引用历史保留，只有 adopted 商品展示资产符合 manifest 白名单。拒绝/取消/失败保持旧图。

任务复用现有 jobs/assets 表，无新增表与 DB11升级。描述符经共享 Application API 自动贯通 Tauri、CLI/MCP；客户端必须拥有具体 operation grant 与 member 范围，不能操作安全凭据。

边界：此 adapter不声明试穿、文本或视觉能力，也不代表其他自称兼容接口能生成。F09/F15/F16/F17后续票据需明确增加专用输入协议与真实验收，不继承商品图验证作为证明。模型发现返回记录只证明供应商可查询的模型清单，不保证账户额度充足；实际成功生成才能确认本次图片调用能力。模型未查询成功时不会提交原图。

依据：
- [Ark 图片生成API](https://docs.volcengine.com/docs/ark/image-generation-api?lang=zh)：Flash不支持组图/流式；省略 sequential_image_generation，2K、b64_json响应。
- [Seedream 模型教程](https://docs.volcengine.com/docs/ark/seedream-4-0-5-0?lang=zh)：Flash ID doubao-seedream-5-0-flash-260915。
