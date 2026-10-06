# 领域术语

- **空间（Space）**：一个设备上由同一 SQLite 数据库承载的衣橱数据边界。
- **成员（Member）**：空间内的独立衣橱身份；单品查询和创建必须明确成员。
- **单品（Clothing）**：真实已有衣物；无图也可创建，不等于购买候选。
- **Application API**：GUI、CLI、MCP 共用的命令和查询边界，输出版本化 JSON 与类型化错误。
- **未知值**：未确认事实使用 null 或空季节列表；不根据空值推断尺寸、价格或日期。
- **最小货币单位价格（purchase_price_minor）**：非负整数，例如 CNY 12900 表示 129.00 元；须同时给出币种。
- **分页游标（cursor）**：单品 UUIDv7 排序位置；结果的 next_cursor 为 null 表示末页。
- **当前成员（Selected Member）**：持久 workspace 记录的 GUI 选择；CLI/MCP 仍须明确 member_id，不将缺失参数隐式补为当前成员。
- **默认成员别名（default）**：旧 CLI 输入兼容标识，通过共用成员解析器映射为 UUIDv7；输出与持久单品均使用 canonical UUID。
- **模型配置（Provider Configuration）**：本机保存的模型端点、模型名、声明能力和认证方式；配置不包含实际凭据。
- **安全凭据（Credential）**：由操作系统安全存储保存的供应商密钥；业务 API 只暴露是否已配置。
- **已验证能力（Verified Capability）**：真实请求验证成功的能力；与用户按供应商文档选择的声明能力分开，文本连接成功不能证明视觉、生成或试穿成功。
