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
- **搭配模板（Outfit / Lookbook）**：可保存、编辑、收藏和克隆的同成员单品组合；保存不表示实际穿过。
- **搭配项（OutfitItem）**：单品外键及独立的顺序、角色和层次，允许任意衣物类别组成至少一件的手工搭配。
- **搭配版本快照（Outfit Version Snapshot）**：不可变的当时组合、单品事实及展示资产引用；后续模板或单品变化不能覆盖旧版本。
- **本地拼贴（Local Collage）**：从保存版本的规范化展示图生成的确定性搭配预览，缺图明确计数，仅本地使用；不等于生成式AI flat-lay。
- **批次（Import Batch）**：一次最多100文件的本机录入集合；批次ID用于读取/处理/归并，不等于逐文件worker任务ID。
- **录入草稿（Import Draft）**：尚未确认入库的单品事实和图片任务引用；默认每文件一草稿，可把完整草稿归并为最多20图的同一单品。
- **持久任务（Job）**：带状态、进度、错误、尝试次数和下一次到期时间的本机处理记录；外部客户端只按operation/member授权访问。
- **worker lease**：短期独占任务的令牌；过期可接管，取消后失效，旧worker不能回写结果。
- **操作标识（Operation Key）**：调用方为同一意图保留的幂等键；批次创建按成员区分，草稿入库按批次区分，不能同键替换输入或入库另一个草稿。
- **派生图片（Derived Image Asset）**：旋转、恢复或抠图产生的新资产；保存原始引用和处理来源，不覆盖旧资产，明确采用后才能替换单品的展示引用。
- **本地抠图模型（Local Cutout Model）**：按固定来源、字节数和 SHA256 校验的本机 U²-NetP 权重；缓存与任务属于本机数据，离线 CPU 推理无需供应商密钥。

- **空间密钥（Space Master Key）**：每个epoch的32字节OS随机主密钥；按空间、用途和epoch派生数据钥，本机存入由系统安全存储中的32字节解锁钥保护的SQLite加密钱包，不能进入普通事实/原图同步。
- **设备确认包（Device Manifest）**：由空间创建设备Ed25519签名的单调revision设备名单及各设备X25519封装钥包；成员档案和客户端授权不等于设备信任。
- **恢复材料（Recovery Material）**：加密的钱包恢复包与单独保存的一次性高熵恢复密钥；恢复包密码可以替代恢复密钥解封，不能与公开同步包共存。
- **待确认设备（Pending Device）**：已认证邀请并生成回执，但尚未取得创建设备签名授权的设备；不能使用空间资料加密解密。
- **商品图生成意图（Product Generation Intent）**：用户明确允许上传所选原图并使用供应商额度的一次持久任务；操作标识去重，不等于已采用的衣物主图。
- **供应商结果未知（Provider Outcome Unknown）**：已提交请求没有可验证结果，且专用图片接口没有查询能力；保留 waiting_input，不能自动或同任务手工重发，重新生成需新的明确意图。
- **真实穿着事件（WearEvent）**：一次明确确认的实际穿着事实，保存当时衣物快照、当地日期、IANA时区及可选模板来源；保存搭配不产生穿着。
- **穿着事实根（root_event_id）**：一场实际穿着的稳定身份；补记或同日换装另建根，更正/撤销追加修订。
- **有效穿着投影**：每根的最新非撤销修订；wearCount按有效根出现次数，wearDays按保存的当地日期去重，独立于UTC创建时间。
- **当地时间精度（time_precision）**：minute表示用户明确分钟；date表示只知道日期，当地中午仅作排序锚点，不是声称实际发生时刻。
- **视觉建议（Recognition Proposal）**：特定供应商、模型、图片版本和单品/批量草稿基线对应的未核实原始描述及规范化候选。与事实独立，人工审核后才写入；重分析不会自动覆盖人工值。
- **识别提交待核实（Unknown Recognition Submission）**：模型 POST 可能已接受而本机未得到完整结果的持久状态。不得自动或通过取消/重试盲重发；明确核实后才建立新意图。
- **AI 搭配图（Outfit Flat-lay）**：用户明确上传保存版本的全部商品图后，由专用图片供应商生成的本机搭配预览；任务保留完整版本与输入资产追溯，采用不改变模板、同步或穿着事实。

### Virtual try-on（虚拟试穿）

人物（本人或有使用权模特）与一个不可变保存搭配版本的全部衣物图，经显式capability图片适配器编辑得到的穿搭视觉参考。输入、provider/model、前图与结果有明确来源；不预测身体尺寸、尺码或真实合身。人物及结果是私有本机job/CAS，不能采用为衣物同步asset，也不自动产生WearEvent。多方案对比各自展示前后图。

- **独立密文文件包（Encrypted Snapshot Package）**：应用在一致数据库快照内选择事实字段和当前／历史采用展示图，按用途加密后写入独立文件夹；配对设备可验证读取，不自动导入事实。供后续同步传输复用。
- **衣橱洞察（Wardrobe Insights）**：当前非删除库存分布与有效实际穿着历史的只读投影；次数/当地天数分别统计，多颜色桶使用明确库存分母，保存搭配不算穿着。
- **久未穿阈值（stale_days）**：成员业务偏好，默认90个自然日，最近实际穿着到查询日差≥阈值才计入；从未穿过单列。偏好持久化并有CAS修订，统计结果不存累加数。
- **实际组合（Actual Combination）**：一个有效穿着事件中的完整衣物ID无序集合，忽略顺序/模板/角色，次数与当地天数独立。
- **洗护事件（CareEvent）**：实际清洗、干洗、维护或手动待洗状态变化的追加事实；更正与撤销生成新修订，旧快照保留。
- **待洗（Dirty / Care Due）**：最近有效清洗后穿着次数达到正整数阈值，或最近有效手动标记尚未解除；不属于单品生命周期。
- **洗护可用性（Care Availability）**：供自动推荐消费的明确 available 与排除原因，待洗不阻止如实手工穿着记录。

## RecommendationIntent（推荐意图）

同成员本地离线控制与当时真实库存组合的不可变记录；包括请求数量、可行数量、固定/排除、角色、缺货与最近七日软避重复放宽说明。替换/换套产生子意图。显式保存会重新验证当前可用性并创建搭配模板，不产生 WearEvent。

- **便携事实备份（Portable Facts Backup）**：一致 SQLite 事实快照与经过清单校验的展示媒体，原图由用户显式选择；普通备份是明文资料包，不携带模型凭据、授权令牌、钱包或设备信任。
- **恢复预备备份（Restore Prebackup）**：明确覆盖前实际保存的原安装事实和可用原图，保留可再次预览/恢复的路径；不是仅在内存缓存旧值。
- **本机安装代际（Installation Generation）**：本机恢复事务递增的并发边界，拒绝恢复前已授权但尚未提交的旧连接写入；便携回退不倒退它，同设备搬迁保留它。
- **数据位置切换（Data Location Switch）**：验证同安装全部本机副本后发布的新数据库位置，旧源保留；待发布标记与进程锁防止阻塞写入落入旧库。

## Weather estimate and AI recommendation

- **Weather estimate**: An explicitly configured member location's provider estimate, with provider valid time, retrieval time, source and attribution. It becomes stale strictly after three hours; off/manual input remains usable when retrieval fails.
- **Validated AI recommendation**: A text model organizes existing rule planner candidates; the application validates current IDs, member, availability, body coverage, roles/layers, fixed/excluded controls and recent wear before publishing an immutable intent. Its explanation is unverified text, not clothing facts.
- **Unknown text submission**: A persisted recommendation job was submitted but its outcome is unconfirmed. Same-key requests do not blindly POST again; explicit offline rules remain available. A confirmed invalid output permits only the bounded second attempt.

## Explicit recommendation feedback

- **推荐反馈（Recommendation Feedback）**：与不可变推荐方案关联的喜欢、不喜欢、跳过、当天排除或明确实际穿着反馈。三维评分为空或1–5整数；意见不隐式产生WearEvent或推断画像。
- **反馈修订（Feedback Revision）**：按稳定root_id追加的更正/撤销，保留原方案与观察时刻快照。撤销意见不撤销真实穿着事实。
- **当天排除（Day Exclusion）**：用户选定单品在保存IANA时区当地日的排除，次日到期，不改变单品生命周期或洗护；固定冲突明确返回。

- **显式偏好（ExplicitPreference）**：用户主动声明的特征权重和说明，可修订/删除，不作为身体尺寸或衣物事实。
- **推断画像（LearningProfile）**：以已知反馈与实际穿着快照可复现计算的版本化排序信号；带算法与源事件修订ID，可查看、覆盖、删除或重置。
- **学习重置边界（LearningResetBoundary）**：用户重置后普通重算只纳入其后记录的事件；完整重建需明确授权，不删除事实。
- **单品锚点配对（Anchor Pairing）**：以一件真实单品为必须出现的固定锚点、离线生成规则搭配建议的本地能力；只产生推荐意图，从不产生穿着事实。
- **配对意图（Pairing Intent）**：以固定锚点生成并保留当时建议的推荐意图；锚点删除或暂不可用后历史仍可读。
- **配对信号（Pair Signal）**：有效反馈与实际穿着中两件单品共同出现所积累的无序共现权重；锚点参与的反馈只学锚点与其余单品的边。
- **配对投影（Pair Projection）**：由与画像重算相同的有效事件集合确定性可复算的配对信号集合，随画像一起版本化并保留来源事件修订列表。
- **过期画像（Stale Profile）**：来源事实变化或来自其他安装后不再参与排序的画像；可查看但须显式重算后才重新生效。
- **着装问答（Stylist Query）**：一次持久化的自然语言问答任务（stylist_query）；先落库返回回执，再由本机任务调用已配置文本模型。任务不可变记录授权来源（可信本机或客户端 grant），回答的解释未经事实核验。
- **只读工具环（Read-only Tool Loop）**：Stylist 任务中模型经服务端白名单工具（衣物查询、规则搭配、推荐意图、确定性统计）访问真实数据的循环；服务端持有全部工具参数，模型不能选择策略、供应商、成员或统计日期，不直接 SQL 或任意文件/网络执行。
- **工具投影（Tool Projection）**：工具结果进入模型提示词前的服务端安全视图；只含白名单字段，封顶数组附带服务器计算的精确总数，备注与内部上下文从不外发。
- **任务预算（Task Budget）**：单次着装问答的模型轮次（8）与工具调用（6）上限；模型输出不合法最多初试加两次重试，超限如实失败；提交结果未知的任务进入等待核对，不盲重发。
- **受控写工具（Controlled Write Tools）**：着装问答中在用户明确指令下触发的记录入口（穿着、洗护记录/更正/撤销、单品编辑/归档）；成员、幂等键、日历日期与时区由服务器注入，模型不能伪造键、跨成员写或指定日期；隐式面（保存搭配、接受推荐、计划）不在白名单，绝不变成穿着。
