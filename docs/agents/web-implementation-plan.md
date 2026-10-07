# Web 实施切片计划（#39，架构草案 ADR0026 的执行边界）

状态：计划草案（2026-10-07），未实现。每个切片先公开行为红测试，再最小实现，产出独立证据（日志/退出码）落备份盘证据目录。执行顺序依用户决定：Web 现在，#25 调度延后，#27 同步在 Web 之后。任何切片不得引入第二套业务引擎或绕过 `execute_as_client`（ADR0001/0007/0026）。

## S0 · 无 Tauri 最小服务启动 + 静态 + 鉴权拒绝
- 范围：loopback Rust 服务骨架（候选 HTTP 栈），服务现有 UI 静态产物与健康端点；无业务代理。
- 红：未鉴权 API 请求返回结构化 401；伪造 Host（DNS rebinding 模拟）返回 403；静态页可加载。
- 证据：服务启动/请求日志、退出码；负向用例输出。
- 依赖：ADR0026 选型评审通过；UI 构建脚本入备份盘（build-storage）。

## S1 · 本机引导与会话
- 范围：首启一次性引导令牌（0600 本机文件）→ 本地管理页恰好一次创建首个 grant；grant token 换 HttpOnly SameSite=Strict 会话 Cookie；登出/撤销即时生效。
- 红：无引导令牌不能创建授权；引导令牌二次使用拒绝；未鉴权/过期会话 401；Cookie 属性断言；令牌不出现在 URL/日志。
- 依赖：S0。

## S2 · 同库只读 + 成员隔离
- 范围：会话 API 只走 `execute_as_client`；只读操作（inventory/category/member.list 等）真实 SQLite。
- 红：限定成员授权读不到他成员数据（member.list 过滤、跨成员 not_found/ member_denied）；CLI 直接读同一数据库读回一致数据。
- 依赖：S1。

## S3 · 授权写入 + CLI 回读 + CSRF/Origin 负向
- 范围：写操作贯通（创建/更新/生命周期）；跨源/null/缺失 Origin 的变更请求拒绝；变更确认令牌。
- 红：合法会话写 → CLI `clothing.get` 读回一致；伪造 Origin/Host 写全部 403；重放由既有 operation_key 语义承担。
- 依赖：S2。

## S4 · 媒体上传/导入/预览
- 范围：浏览器→loopback 上传入有界临时文件（类型/大小/遍历校验、崩溃清理），随后既有 `assets.import`/`imports.create`；预览仅 `assets.preview` 成员授权。
- 红：超大/坏类型/路径遍历上传拒绝且清理；导入后 assets.get/CLI 读回一致；未授权成员预览拒绝；任意服务器路径参数被拒。
- 依赖：S3。

## S5 · 特权本地管理代理
- 范围：服务进程内有界本地管理页（引导/登记/撤销客户端、凭据、加密、`schedule.runtime.configure`），精确动作白名单 + 本地在场确认。
- 红：通用会话 API 调用 `client.*`/`provider.credential.*`/`encryption.*`/`schedule.runtime.configure` 一律拒绝（与 registry scope 元数据一致）；代理外无任何可信通道；凭据仅经 stdin/OS keyring，不进日志。
- 依赖：S1。

## S6 · worker / 备份 / 数据迁移
- 范围：既有 job/回执/租约复用（imports/images/learning 等）；备份/恢复/`data_directory.move` 经本机适配器 + 全成员授权 + 独占写 + 预备份；服务关闭/休眠限制如实呈现。
- 红：任务失败/取消有真实回执；非全成员授权备份拒绝；迁移后 Web/CLI/MCP 读同一新位置；无伪造完成状态。
- 依赖：S3、S5。

## S7 · F01–F30 Web 覆盖（现有四工作区 UI）
- 范围：经适配器贯通库存/分类、多图录入、穿搭、穿着/洗护、洞察、推荐、已实现 AI 与设置；320/375/414/768px、触摸/键盘、空/加载/错误/离线状态。
- 证据：逐项 F01–F30 Web 实现与验收记录；未完成项与浏览器限制如实标记；不用桌面通过结果替代。
- 依赖：S0–S6 按功能依赖推进。

## S8 · 三 OS 浏览器验证 + 发布/回滚文档
- 范围：macOS/Windows/Linux 真机浏览器关键流程证据；部署/升级/回滚与已知限制文档；原生依赖各平台打包证据。
- 证据：每 OS 实测记录；候选浏览器基线验证结果。
- 依赖：S7。

调度（#25）与同步（#27）按用户顺序分别于 Web 交付后推进；本计划不含其切片。
