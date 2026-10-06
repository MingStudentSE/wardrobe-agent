# #12 授权基础验收

2026-10-07，macOS Apple Silicon。分支 codex/ticket-12-auth，起点6743264，合入integration1ca5d8a（媒体UI、数据性能及旋转证据）。

真实Application/SQLite行为：

- runtime red→green：client.authorize原先unsupported_operation；有效限定token原先unauthorized；client.list/revoke原先unsupported_operation；遗漏member_ids原先扩大为全部成员，修正为invalid_input；无授权CLI原先成功读库存，修正为unauthorized；损坏授权文档原先遗漏成员范围后扩大，修正为storage_error。
- 限定token读取授权成员成功，写入超scope拒绝，另一成员拒绝；member.list过滤成员和当前选择，capabilities过滤scope。
- 完整token重复写入两件衣物成功；客户端不能调用client.authorize或provider.credential.status提权；也不能通过provider.save端点/认证变化间接清理密钥，锁内策略由独立runtime red→green测试验证。
- revoke重启后仍拒绝；真实等待到期后拒绝，不修改内部时钟或mock。未知/损坏授权不放宽范围。
- 令牌只在登记输出一次；list无token/hash；SQLite/WAL文件字节扫描无原始令牌。
- 真实CLI --client-stdin（token不在argv）和独立MCP都执行相同scope/成员限制；无token MCP拒绝。MCP initialize一次授权后两次写入成功，不重复登记/确认；tools/list隐藏本机授权管理能力。stdin/stdout错误输出无token。
- 原CLI行为测试改为显式--trusted-local人工维护；MCP行为测试改为真实登记的token。默认外部入口没有隐式可信fallback。

GUI：

- 真实Chrome+开发桥+CLI backend：登记只读客户端，一次性password令牌关闭后消失；撤销后重载保留，无重新读取令牌；过期输入显示错误，修改后完整业务/全部成员明确选择并登记成功。
- 320/375/414/768/1024/1280/1440宽，无水平溢出。截图client-settings-320.png / client-settings-1280.png，关闭令牌后截图；不保存令牌画面。
- 已有member测试定位精确combobox，防止新scope checkbox的“切换当前成员”描述造成宽泛label歧义，原先成员切换、保存竞态行为保持断言。
- scripts/review-media.py / GUI开发桥 / Playwrightfixture都显式使用--trusted-local。开发桥拒绝跨Origin请求。

运行：cargo test --workspace；cargo fmt --all -- --check；cargo clippy --workspace --all-targets -- -D warnings；npm run build；真实Chrome E2E全部通过。原生HEIC测试env沿用/tmp/wardrobe-media-native/install的pkgconfig及dylib。

安全边界、后续模块接入规则见ADR0007。未实现同步；后续新CLI/MCP适配器必须用execute_as_client，新的空间级操作默认对限定成员拒绝。当前基础满足本机授权slice，真实供应商及其他平台安全存储门槛由root继续验证，#12未由此自动关闭。
