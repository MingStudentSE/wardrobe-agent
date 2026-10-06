# 领域术语

- **空间（Space）**：一个设备上由同一 SQLite 数据库承载的衣橱数据边界。
- **成员（Member）**：空间内的独立衣橱身份；单品查询和创建必须明确成员。
- **单品（Clothing）**：真实已有衣物；无图也可创建，不等于购买候选。
- **Application API**：GUI、CLI、MCP 共用的命令和查询边界，输出版本化 JSON 与类型化错误。
- **未知值**：未确认事实使用 null 或空季节列表；不根据空值推断尺寸、价格或日期。
- **最小货币单位价格（purchase_price_minor）**：非负整数，例如 CNY 12900 表示 129.00 元；须同时给出币种。
- **分页游标（cursor）**：分页查询返回的续页位置；单品列表使用当前筛选与排序内的单品 ID，历史使用事件位置。调用方须保留查询条件，next_cursor 为 null 表示末页。
- **当前成员（Selected Member）**：持久 workspace 记录的 GUI 选择；CLI/MCP 仍须明确 member_id，不将缺失参数隐式补为当前成员。
- **默认成员别名（default）**：旧 CLI 输入兼容标识，通过共用成员解析器映射为 UUIDv7；输出与持久单品均使用 canonical UUID。
- **模型配置（Provider Configuration）**：本机保存的模型端点、模型名、声明能力和认证方式；配置不包含实际凭据。
- **安全凭据（Credential）**：由操作系统安全存储保存的供应商密钥；业务 API 只暴露是否已配置。
- **已验证能力（Verified Capability）**：真实请求验证成功的能力；与用户按供应商文档选择的声明能力分开，文本连接成功不能证明视觉、生成或试穿成功。
- **本机客户端授权（Client Grant）**：可信本机用户登记的外部客户端权限，包含实际 operation scope、明确成员范围和到期时间；不等于空间协作角色。
- **连接令牌（Client Token）**：256bit 随机本机授权凭据，一次性返回，SQLite仅保留hash；与供应商API key分开。
- **可信本机维护（Trusted Local Maintenance）**：GUI及显式 --trusted-local 人工维护入口，等同于拥有本机数据目录的权限；不用于外部客户端连接。
