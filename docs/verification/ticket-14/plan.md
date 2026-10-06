# #14 F07 验证范围与实施计划

公共验证边界沿用已确认的 Application API（真实 SQLite 与文件）、真实回环 HTTP 协议服务器、CLI/MCP、GUI 驱动真实共享 API。协议 fixture 只证明适配器行为，不证明供应商账户已开通、付费调用或衣物生成质量。供应商 API key 只由用户安全设置输入，不读取既有个人密钥。

专用 `ark_seedream` adapter 声明 `image_generation`，不使用 chat/completions 假装生成。默认模型 `doubao-seedream-5-0-flash-260915`；模型名和最终端点可配置，具体 family 决定输入和参数。模型发现先查询 `/models`，如果供应商没有暴露该接口须用户在控制台确认开通，不能把 404 自动解释为实现错误。

持久任务复用 jobs。先 enqueue，再处理；请求发出前持久 submitted_at 标记。同步图片接口没有公开查询 API；超时、失联、进程退出后请求结果未知则 waiting_input，保留请求标识与固定错误并拒绝同任务重发。用户显式确认新的生成意图可能再次计费，不将重试按钮用作未知结果自动重发。取消会作废 lease，迟到响应不能创建或采用结果。

生成结果经有界读取、实际图片解码、方向归一和1600最长边 WebP 压缩生成 preview 新 Asset ID，保留原始来源和 provider/model/request provenance。衣物事实、旧图不变。明确采用才替换当前主图及同步资格；拒绝、取消、失败不修改旧图。

官方来源（2026-10-07核对）：
- https://docs.volcengine.com/docs/ark/image-generation-api?lang=zh
- https://docs.volcengine.com/docs/ark/seedream-4-0-5-0?lang=zh
- https://api.volcengine.com/api-explorer/debug?action=ImageGenerations&groupName=%E5%9B%BE%E7%89%87%E7%94%9F%E6%88%90API&serviceCode=ark&version=2024-01-01

Flash/Pro 单图不支持 sequential_image_generation、流式或组图。输入单张≤30MB、≤3600万像素、每边>14px、比例1/16–16；最多10参考图。F07首次只允许一张明确选定的单品图；不得上传其他成员或全部照片。2K输出尺寸由模型处理。实现不会声称这些限制是适用于试穿等其他 capability 的通用规则。

待实际供应商验收：安全输入key→确认可用模型→发送公共或明确授权衣物照片→查看实际结果颜色/版型/印花→明确采用→重启确认。没有这份证据不会关闭 #14。
