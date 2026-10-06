# #12 显式文本 / 视觉连接验证

基于 integration 9b30572，复用真实 Application、SQLite、CLI/MCP、GUI 和现有 Atelier 视觉系统。API `provider.validate` 必须携带 `id` 与 `capability`（text / vision）；未声明或专用图像生成/试穿能力在网络请求前拒绝。共享 descriptor 同时定义 CLI/MCP contract。

视觉验证发送 OpenAI-compatible `/chat/completions` 的真实多模态 content：文字及内置64×64 PNG data URL，低细节、最多32个输出 token。该图为代码生成的原创蓝色方块、浅色背景，无用户图片、文件路径或元数据。每次仅验证所选能力，不自动重试、不自动验证其他能力。供应商响应须为兼容JSON且含非空文字；返回 verified_capabilities 只含本次能力，成功不是视觉质量认证。失败继续使用固定错误，不输出响应正文或凭据。

GUI 为已保存模型提供“待验证能力”选择及“验证所选能力”，视觉专用配置可直接选择视觉；明确说明一次调用可能消耗供应商额度、使用内置图和能力声明/成功响应的边界。保存或选择配置不会联网验证。限权客户端仍须已有 provider.validate 授权，不能借此读取凭据。

## 验证

- TDD red：新测试要求缺少显式能力返回 invalid_input，旧实现实际返回 unsupported_capability，运行时失败后实现修复。
- Core 真实 SQLite + loopback HTTP：文本返回仅 text；视觉专用配置发送可实际解码为64×64 PNG的多模态请求，返回仅 vision；未显式选择及未声明能力在联网前拒绝。Provider tests 5通过，1系统凭据测试本次未重跑。
- CLI/MCP 独立进程：同一 SQLite 保存视觉专用模型，可信维护路径登记仅 provider.validate 的限定客户端；token 仅经 stdin 传递，CLI 和 MCP 都向真实 loopback HTTP 发多模态图片请求并返回 vision。新增测试通过。
- `cargo test --workspace` 通过；本次新增 CLI测试另行通过。
- TypeScript + Vite production build通过。
- Playwright real Chrome、真实GUI→开发bridge→CLI→Application→loopback HTTP：2通过。保存前后0验证请求，用户点击后恰好1；全部7个规定宽度无水平溢出。已人工查看320px截图，视觉语言一致、主要按钮与表单完整可触达。

证据：[320px](model-settings-320.png)、[1280px](model-settings-1280.png)。Loopback是协议/产品全链路验证，不是真实供应商账户成功。真实供应商文本和视觉模型验证仍需用户在安全界面配置其账户并显式发起；本次未读取用户密钥，未声称已通过真实供应商。图片生成/试穿专用适配器属于后续票据。
