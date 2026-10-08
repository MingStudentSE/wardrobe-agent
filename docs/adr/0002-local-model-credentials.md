# ADR 0002：模型配置与系统安全凭据

状态：已采用（2026-10-07），#12；真实供应商/三平台验收仍需后续记录。

模型端点、模型名、声明能力和认证方式持久在本地 providers 表，初始化检查实际表存在，兼容并行迁移，不单独提升全局数据库版本。表不写 outbox；凭据也不进入单品事实、同步、导出或审计。配置写入使用共用 IMMEDIATE 事务与锁后版本检查。

凭据通过 keyring 3.6.3 的明确平台 feature：macOS Keychain、Windows Credential Store、Linux D-Bus Secret Service + crypto-rust。服务名 wardrobe-agent.provider；account 为 provider UUID 与端点 SHA256，避免端点并发修改时把另一端点的密钥发送给旧端点。端点变化或改成无认证时移除旧凭据；系统存储不可用则明确失败，禁止明文和 mock 回退。OS 不可用不阻断无认证的本地模型设置或手工录入。

provider.credential.set/remove/status 只返回配置状态，不提供密钥读取工具。CLI 写密钥必须用 --input-stdin，开发浏览器桥也用子进程标准输入，避免把输入放进进程参数。MCP/Tauri 在同一 Application API 调用；之后 #27 必须对凭据操作执行客户端 scope 授权。这里不将尚未实现的客户端授权判为已完成。

OpenAI-compatible 适配边界当前接受 text/vision 声明能力。provider.validate 必须显式选择 capability=text 或 vision。文本发送最小 chat/completions 请求；视觉发送同一接口的 text + image_url 多模态请求，图片为应用原创内置的64×64无元数据蓝色方块 PNG、detail=low、max_tokens=32，不读取衣物或用户文件。每次只调用所选能力，不默认额外验证或耗费额度；20秒超时、不自动重试、不追随重定向。只报告本次实际验证的能力，声明能力不等于连接验证，成功响应也不保证任务质量。网络及供应商错误输出固定错误，不返回可能回显密钥的响应正文；用量仅返回供应商提供的非负整数 token 数，不推算价格或余额。图片生成与试穿须由后续专用适配器提供真实调用，不能靠能力复选框宣称支持。

GUI 凭据输入保存后清空；验证/密钥操作须使用已保存配置。更换密钥是专门按钮。没有收费入口。

依据：[keyring 平台 feature 官方文档](https://docs.rs/keyring/3.6.3/keyring/)。Linux发行包须提供 libdbus，桌面运行必须有已解锁 Secret Service；干净安装和实际凭据 smoke 属发布门槛。
