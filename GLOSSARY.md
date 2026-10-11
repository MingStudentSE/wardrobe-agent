# 领域术语

- **空间（Space）**：一个设备上由同一 SQLite 数据库承载的衣橱数据边界。
- **成员（Member）**：空间内的独立衣橱身份；单品查询和创建必须明确成员。
- **单品（Clothing）**：真实已有衣物；无图也可创建，不等于购买候选。
- **Application API**：GUI、CLI、MCP 共用的命令和查询边界，输出版本化 JSON 与类型化错误。
- **未知值**：未确认事实使用 null 或空季节列表；不根据空值推断尺寸、价格或日期。
- **最小货币单位价格（purchase_price_minor）**：非负整数，例如 CNY 12900 表示 129.00 元；须同时给出币种。
- **分页游标（cursor）**：分页查询返回的续页位置；单品列表使用当前筛选与排序内的单品 ID，历史使用事件位置。调用方须保留查询条件，next_cursor 为 null 表示末页。
- **全链路日志（Full-chain Log）**：数据目录 `logs/` 下的 JSONL 追加文件，独立于 SQLite，按天分文件并有单文件/总量/保留天数上限；按构造排除凭据与隐私内容，写失败静默计数不阻塞业务。
- **链路标识（trace_id）**：每次 Application API 调用分配的 UUIDv7 编号，贯穿同步操作、其创建的持久任务、worker 执行与供应商请求；错误输出附带它用于精确回查日志。
- **运行会话（run_id）**：每次应用启动生成的会话标识，所有日志事件携带；诊断页按「本次启动」切分查看与导出，跨重启导出按时间窗覆盖上次运行。
- **基础检查（Diagnostics Checks）**：GUI 诊断页与 CLI `doctor` 共用的只读体检清单（数据目录可写、数据库可打开且模式版本一致、WAL、日志可写、磁盘空间、安装代际、加密状态、模型凭据、卡住任务、待发布位置标记），每项 pass/warn/fail 附通俗说明与建议去向；只诊断不自动修复。
- **诊断摘要（Diagnostics Summary）**：只读健康投影——数据库模式版本、本机安装代际、按状态任务计数、供应商结果未知数、最近错误、日志占用与丢弃计数，以及日志目录实际路径（`logs.directory`）。
- **诊断文件（Diagnostics Export）**：单文件 JSONL 导出；首行 meta（应用版本、系统、数据库模式版本、时间范围、运行会话列表、按级别计数、最近错误摘要），其后为按时间排序的日志事件；不含数据库、图片原图与任何凭据。
- **复现记录（Reproduction Recording）**：诊断页的窗口化记录——开始后正常使用应用，页面切换/对话框/操作结果以结构化事件进入操作时间线，停止并导出只含该时间窗的单文件；最长 10 分钟自动停止，脱敏与普通日志同一套构造。
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
- **提交台账（Submission Ledger）**：`submission.rs` 承载的「登记已提交 / 判定结果未知 / 放行有限重试」单一实现——任务在真实发送前于台账登记，重试守卫（jobs.retry、worker 入口、恢复守卫）与诊断投影的「已提交」谓词全部从它导出；内部为图像与文本轮次保留两种持久字段编码（显式适配），对外只见同一种「已提交」判定。
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

- **定时计划（Schedule Plan）**：成员事实，保存每周执行日（ISO 1–7）、本地时刻、IANA 时区、场合、当天/次日目标、执行设备、启用状态、补跑窗口与修订；修改只对未来生效，暂停即停排队。
- **执行设备（Execution Device）**：计划声明的唯一执行本机身份；本机设备ID不属于可移植业务事实，恢复备份后须显式改派才由此设备执行。
- **补跑窗口（Catch-up Window）**：到期后仍允许补执行的时长，默认最近120分钟，可配置1–1440分钟；恢复时每计划只补最近一次，更早时段记missed，窗口外直接标错过。
- **执行游标与本地收据（Evaluation Cursor & Local Receipt）**：本机记录的每计划评估位置和已登记执行的安全收据；用于跨重启、跨恢复去重，不属于可移植事实。
- **通知提交状态（Notification Submission Status）**：通知outbox的真实系统提交记录——pending/submitting/submitted/unknown/failed；submitted只表示系统接口接受，不表示用户看到；unknown与崩溃后不得盲重发，仅明确failed可显式重试。

## Local-folder dual-device sync

- **删除墓碑（Delete Tombstone）**：单品删除以 `status=deleted` 与 `deleted_at` 的完整事实修订进入同步日志；对端按序应用后同样进入回收站，恢复修订可再次激活。回收站单品不出现在默认列表；全新设备只引导当前事实，已删除历史不复活。
- **受保护分类删除（Protected Category Delete）**：默认分类不可删除；删除自定义分类先在同一事务把引用它的单品迁入其他并保留审计，再写分类墓碑。对端按序收到迁移与墓碑后分类列表与源端一致。
- **资产发现预算（Asset Discovery Budget）**：每次同步批对每个 blob 目录最多消费固定页数的列举，位置持久化并在耗尽时回卷；已登记的 blob commit 键不再重复解密。已处理提交事后损坏不阻塞后续同步，未处理提交损坏仍整体失败。
- **发布标记门槛（Publication Marker Gate）**：只有 `.commit` 标记可见的对象才会被对端发现；对象损坏或被截断时同步整体失败，不应用该对象的部分字节，修复后同一日志继续收敛。
- **条件写入能力探针（Conditional-Write Capability Probe）**：WebDAV 等外部传输在连接与配置期用真实条件写入握手验证服务器确实执行防覆盖判定；未执行的服务器被明确以 storage_unsupported 拒绝，不静默降级。
- **传输条件令牌（Transport Conditional Token）**：WebDAV 适配器用服务器 ETag（If-None-Match/RFC 4918 If 头）做条件写的唯一依据；ETag 只是透明令牌，绝不当作内容摘要，内容完整性始终由 sha256 发布标记判定。

## S3-compatible sync backend

- **同步传输适配器（Sync Transport Adapter）**：实现统一 `SyncStorage` 契约（能力声明、分页列举、stat/get/put/delete）的传输后端；本地文件夹与 S3-compatible 对象存储共用同一契约，切换适配器不改变加密协议、包白名单与领域结果。
- **条件写探测（Conditional Write Probe）**：适配器构造期对真实服务器验证 `If-None-Match:*`（CreateOnly）是否被强制执行；服务器忽略条件头或不支持时以 `storage_unsupported` 明确拒绝，绝不静默改写发布语义。
- **ETag 版本（ETag Version）**：S3 对象的 ETag 只作为条件写协调（`If-Match`/`If-None-Match`）的版本标识，绝不当内容 hash；完整性始终由同步协议的 sha256 摘要（对象→marker 两步发布与读取校验）承担。
- **重试预算（Retry Budget）**：网络错误、超时与 5xx（含 503 SlowDown）在首试后最多自动重试 2 次并遵守 Retry-After；预算耗尽映射 `storage_transient`。认证失败（401/403）映射 `storage_auth`，缺失对象映射 `storage_not_found`，不混用。
- **同步目标凭据（Sync Target Credential）**：S3 目标的访问密钥只保存在 OS 安全存储（keyring），按目标键分账户；配置输出、事实库、日志与同步包字节中不得出现。

## Purchase candidate domain（购买候选域）

- **购买候选（Purchase Candidate）**：用户想买的商品草稿，存放在独立候选域（`domain=purchase_candidate`，独立 UUIDv7 身份），与真实衣物严格区分；不计入库存、穿着、洞察或推荐，不进入 outbox（不同步、不入便携备份），恢复时与识别建议一样按本机草稿清除。价格与尺码等未知事实保持 null，不编造。
- **候选属性建议（Candidate Suggestion）**：候选商品图经视觉供应商的未核实描述与保守规范化候选；写入候选 provenance 供人工对照，绝不自动覆盖人工事实；提交前须明确确认发送图片，提交结果未知时持久化待核实，不盲重发。
- **候选纠错（Candidate Correction）**：人工逐项核对后显式写入候选事实的动作（confirm_confirmation）；人工值优先，建议值保留在 provenance 供对照。
- **购买适配分析（Purchase Fit Analysis）**：对候选的一次确定性只读分析——搭配潜力（候选固定占据其角色、覆盖角色只来自当前可用的真实库存，组合总数为各覆盖槽位库存数的精确乘积）、重复购买风险（同类别库存逐项按颜色/风格/场合/季节 Jaccard 加权，千分制权重 400/250/200/150，≥700 且≥2 维度可比记高度重合）与缺口分析（内置类别与四场景的可用库存计数，数量为 0 即空洞；候选填补声明只在精确相等时作出）。结果版本化保留，缺失事实如实列为不确定性；creates_wear=false、counts_toward_inventory=false、不自动购买。

## Scenario & travel plans（场景与旅行计划，issue #23）

- **场景计划（Scenario Plan）**：成员事实，婚礼/面试单事件或多日旅行的持久化搭配安排（scenario_plans）；基于当时真实可用库存逐天生成的领域工件，`creates_wear` 恒为 false，结构性不进入 wear 面——计划、查看、修改与保存都不记实际穿着。
- **单事件计划（Event Plan）**：一个当地日期的完整方案，场合为婚礼或面试，按正式场合规则适配；只采用单品已记录的场合标签，不推断未知事实。
- **旅行计划（Travel Plan）**：2–14 天的逐日方案；贴身/主要衣物类别（top、bottom、dress、one_piece、sportswear、underwear）不连续两天重复，鞋、外层、包、配饰可跨天复用。
- **去重打包清单（Deduped Packing List）**：全部逐天方案按衣物聚合的清单，每件衣物只出现一次并列出实际使用的天；跨天复用带 `reused` 标记，相邻天重复带 `consecutive_repeat` 标记，绝不重复计数或虚构库存。
- **计划固定/排除（Plan Fixed/Excluded）**：固定单品必须真实落位（归属、active 生命周期与洗护可用性服务端校验，落位失败如实说明），排除单品永不出现；修改计划时显式排除自动解除同单品的固定。
- **逐天轮换降级（Day Rotation Fallback）**：库存不足以维持「不连续重复」时，允许贴身衣物连续穿着并把「库存不足：…需连续两天重复穿着」写入短缺原因；缺少角色/无解给明确原因，不静默重复或虚构。
- **计划模型辅助（Plan Model Assist）**：strategy=ai 时先持久化 scenario_plan 任务并返回回执，再由本机 worker 经共享 provider 文本通道请求模型；模型逐天指派须通过与离线规则相同的服务端校验（真实ID、归属可用、固定/排除、逐天轮换、基础覆盖），失败最多三次尝试后按本地规则降级并如实写入失败码与模型反馈；提交结果未知进入等待核对，不盲重发。

## Sync conflict and event consistency

- **因果支配（Causal Dominance）**：并发事实用每操作因果向量钟（base/clock）判定支配关系；被支配的旧候选自动收敛，互不支配的并发候选全部保留。本地墙钟、到达顺序与操作号顺序（随机 op_id 的稳定排序）都不裁决事实；未解决时的暂定显示只是稳定一致的预览，须显式解决或等因果更新覆盖。
- **字段变体冲突副本（Field Variant Conflict Copy）**：同字段并发不同值在 `sync_fields` 以变体（值+因果钟+操作号）双方保留；`sync.conflicts` 对外可见（含预期令牌），`sync.resolve` 以 `expected_token` 乐观并发与幂等操作键显式选择，旧历史保留。
- **集合操作语义（Set Operation Semantics）**：颜色、标签等集合字段按元素存在性（present/label）登记，不做整值覆盖；并发的两个添加都不丢，同元素并发添加只计一次，同元素并发一增一删按“添加可见”合并。
- **删改冲突副本（Delete–Edit Conflict Copy）**：删除与离线编辑并发时，删除在裁决中优先（迟到编辑不复活已删事实），编辑值保留为 lifecycle 冲突副本；`sync.resolve` 的 lifecycle 显式恢复（value=active）保留并发编辑，恢复永远不会自动发生。
- **搭配结构冲突（Outfit Structure Conflict）**：并发修改搭配 items 以变体双方保留并在同步设置面板可见；解决方式是显式重新发布搭配版本，并发版本历史继续经 `sync.history` 可查。
- **事件乱序挂起（Out-of-order Staging）**：因果前序（依赖实体或因果钟）未到达的事件进入 pending 挂起重放，不静默丢弃也不阻塞独立事实流；重复投递（同 op_id 重复标记）按 inbox 幂等只应用一次，穿着次数按有效事件、天数按当地日期去重的既有口径不变。
- **同步不回声（No Sync Echo）**：对端事实经 inbox 收据与发布标记只应用并转发一次；冲突解决、恢复与画像重算写入审计与同步回执等本机记录，不生成同逻辑同步日志（outbox 已停写，见 ADR-0040）。收敛后重复同步 sent=received=0，两空间画像重算结果一致。

## Sync operations and key lifecycle（issue #31）

- **同步暂停（Sync Pause）**：本机主目标文档内的持久开关；`sync.run` 入队与 `sync.process` 领取都以 `sync_paused` 显式拒绝，暂停前已入队任务保留、恢复后同一任务继续。暂停只作用于本机目标，不向对端传播。
- **目标切换复制校验（Target Switch Copy Verification）**：更换主目标先把已发布对象完整复制到新目标并逐对象 sha256 回读校验，全部通过才在一个事务里切换主目标并搬迁按目标键的传输状态；任一步失败保持旧目标（回退 = 旧目标原样可用），旧目标字节永不清理。
- **单调对象向前合并（Monotonic Forward Merge）**：发布者页游标 `head.index` 与设备水位记录允许在复制对账中「向前合并」（解密双方取更前进者），同页码不同摘要仍视为损坏冲突；其余对象字节不一致一律整体中止。
- **镜像备份（Mirror Backup）**：主目标的单向副本（`sync.mirror.configure/sync/promote/remove`），不参与多主协调；增量补齐幂等。显式提升前校验镜像与主目标逐对象一致且发布标记与对象字节自洽，旧主目标原样保留、可再次提升切回。
- **设备消费水位（Device Consumption Watermark）**：每设备对每个发布者已完整消费的连续索引页位置（`sync_index_read.next_page`），同步步末以加密记录发布到 `{space}/epochs/{e}/watermarks/`；op_id 不保证时序，页位置才是单调可靠的消费进度。
- **安全回收三条件（Reclaim Triple Gate）**：远端改动日志（tombstone/已发布提交）只在「全部当前配对设备水位覆盖该页 + 保留窗口已过（默认 30 天，可配置）+ 调用方确认存在可恢复备份基线」时按页整页回收（标记先删、对象后删）；缺任一条件如实列出被阻塞设备，绝不按固定时间盲删。晚于窗口加入的全新设备须先恢复备份基线再同步前推。
- **已处理键登记回收（Processed-Key Registry Reclaim）**：`sync_asset_commits` 的已处理 blob 键在远端对象消失后随回收扫描清除死行（每批有界），登记不再永久膨胀；远端 blob 对象本身不回收——新设备引导与恢复备份依赖目标上完整的展示图 commit。
- **凭据随键迁移（Credential Migration on Epoch Roll）**：纪元推进滚动目标键时，WebDAV/S3 传输凭据自动迁移到新键（旧键保留供回退），无需重输密码；凭据只存 OS 安全存储，永不进入配置、日志或同步包。解除配对与恢复备份会清理这些凭据并清空按目标键的传输状态。
- **撤销致盲与自锁（Revocation Blinding & Self-Lock）**：撤销即纪元轮换——所有者新事实只进入新纪元命名空间，被撤销设备停留在旧纪元视图、不再取得任何新事实；设备状态协议送达新清单后自锁（`revoked`），此后同步以 `device_revoked` 显式拒绝。改密码只重包装本机钱包，不影响空间密钥与同步；错误密码恢复失败不破坏现有状态。
- **最近失败如实呈现（Last-Failure Outcome Gating）**：`sync.status.last_failure` 只在最近一次同步结果是失败时呈现；失败批次显式重试成功后恢复干净状态，历史失败不纠缠后续操作。
- **全链路日志（Diagnostics Log）**：独立于 SQLite 的结构化 JSONL 追加日志，按天写入数据目录 `logs/` 子目录；单文件、总占用与保留天数封顶，写入尽力而为、失败静默降级并计数。
- **链路标识（trace_id）**：每次 Application API 调用分配的 UUIDv7；同一次调用的日志事件与错误输出共享此值，是用户反馈与日志查询的精确锚点。
- **运行会话（run_id）**：每次进程启动生成一次的标识；跨天、跨文件的同一运行以 run_id 关联，日志初始化进程内幂等（GUI 每命令重开 Application 不产生新 run_id）。
- **诊断摘要（Diagnostics Summary）**：仅本机可信入口的只读健康投影——数据库模式版本、本机安装代际、按状态任务计数、供应商结果未知计数、最近错误、日志占用保留信息与丢弃计数（含 `logs.directory` 日志目录路径）；与持久任务实际状态一致。
- **基础检查（Basic Checks）**：仅本机可信入口的只读体检清单（`diagnostics.checks`，与 CLI `doctor` 同源）——固定十项各报 pass/warn/fail、通俗中文消息、建议去向设置页标识与技术明细；只诊断不自动修复，不修改任何配置或数据。
- **复现记录（Reproduction Recording）**：诊断页「开始记录复现 → 正常使用 → 停止并导出」得到的单文件窗口导出——窗口起止为显式标记事件（`diagnostics.reproduction.start/stop`），meta 标 `kind: reproduction` 并附记录时刻的基础检查结果；最长 10 分钟由前端计时自动停止，录制状态不持久化（刷新页面即放弃），记录不改变任何日志级别或脱敏行为。
- **诊断文件（Diagnostics File）**：`diagnostics.export` 产出的单文件 JSONL——首行 meta（应用版本、操作系统、数据库模式版本、导出时间、覆盖时间范围、run_id 列表、按级别计数与最近错误摘要），其后为按时间排序的日志事件；范围支持本次启动 / 最近 24 小时（跨重启）/ 保留期内全部或显式起止时间窗（含端点）；产物只来自日志文件与摘要投影，天然不含数据库、图片与凭据，与普通日志同一脱敏构造。
- **用户操作时间线（UI Event Timeline）**：前端关键交互（视图切换、对话框打开/提交）经 `diagnostics.ui_event` 写入全链路日志的事件序列（event_type=ui）；仅接受封闭白名单事件类型与简短界面文案标签，不接受自由文本，仅本机可信入口。
