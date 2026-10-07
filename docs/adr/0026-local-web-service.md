# ADR0026 · 本机 Web 服务架构（草案）

状态：草案（2026-10-07），#39 架构准备；仅源码观察与提案，未实现、未验收。执行顺序依用户决定：Web 现在，#25 调度延后，#27 同步在 Web 之后；macOS 桌面 Beta 保留，Windows/Linux 桌面包延后。

本文严格区分三部分：**观察到的现状**（引用实际源码）、**提案**（候选方案与安全设计，未实现）、**未决验证门**（不得当作已支持）。

## 一、观察到的现状（源码事实）

- 共享边界：`crates/wardrobe-core/src/lib.rs` 的 `Application::execute` / `execute_as_client` 是唯一公开命令/查询边界（ADR0001）；`default_database_path()` 提供三平台一致数据目录与 `WARDROBE_DATA_DIR` 覆盖；bundled SQLite + WAL + IMMEDIATE 迁移事务。
- 外部授权：`crates/wardrobe-core/src/authorization.rs` 的 `execute_as_client` 先查 token hash、撤销/到期、精确 operation scope、成员范围，再入业务；`privileged()` 将 `client.*`、`provider.credential.*`、`encryption.*` 限定可信本机；ticket25 合并新增精确名特权操作 `schedule.runtime.configure`（registry 契约测试已按精确相等维护）。令牌仅在登记时返回一次，库内只存 SHA256，复用语义为到期/撤销前有效（ADR0007）。
- CLI/MCP：`crates/wardrobe-cli/src/main.rs` 的类型化解析 + `mcp()` 传输直接打开同一数据库，外部模式经 `--client-stdin`（ADR0007）；registry 元数据（scope/effect/examples）由 `operations()` 单源派生（ADR0025 及 registry 契约测试）。
- 桌面桥现状：`apps/desktop/src/api.ts` 仅 5 行——DEV 且非 Tauri 环境 fetch `/__wardrobe_dev_api`，否则 Tauri `invoke('application')`。`apps/desktop/vite.config.js` 的 `/__wardrobe_dev_api`、`/__wardrobe_dev_upload`、`/__wardrobe_dev_batch_upload` 是开发专用中间件（不进生产 bundle）：对每个请求 spawn `wardrobe-cli --trusted-local <operation> --input-stdin`，Origin 检查仅 `origin===http://host` 且允许缺失 Origin，上传写系统临时目录（20MB/64MB 上限，finally 清理）。
- Tauri 壳：`apps/desktop/src-tauri/src/main.rs` 的 `application` 命令可信直调 `Application::open(default_database_path()).execute`；`search_weather_city` 是壳内唯一出网请求（open-meteo 地理编码）；对话框经 `tauri_plugin_dialog`。
- 结论（事实，非提案）：现有开发桥不是生产 Web——每命令子进程、全量 `--trusted-local`、无会话/CSRF/DNS-rebinding 防护、允许任意 operation 透传；生产 Tauri 路径依赖 Tauri 运行时。两者都不能直接当作跨平台 Web 交付。

## 二、提案（候选方案，实现未开始）

### 选型：独立同源 loopback Rust 服务（候选）

推荐**独立本机 Rust 服务**：直接链接受支持平台上已部署验证的 `wardrobe-core`，同进程持有一个 Application，服务静态化的现有 React 四工作区 UI 与同源 JSON API；绑定 `127.0.0.1` 固定端口配置。无公网后端、无账号体系、无默认远程监听、无照片上传互联网默认。

- 与 Tauri 桥对比：Tauri 方案要求用户先装桌面壳，违背"三 OS 浏览器可用"的验收；开发桥按命令 spawn 进程无法承载会话与上传语义。两者都不选为 Web 主路径。
- 与浏览器本地（WASM/OPFS/IndexedDB）对比：浏览器源内存储是浏览器拥有、按站点隔离的沙箱，**当前 Rust SQLite/CLI/MCP 无法打开**；走此路需要验证 Core 的 WASM 可移植性，并新增 SQLite/媒体/凭据/worker 适配与浏览器存储访问机制；不能把 IndexedDB 中另写库存/统计/推荐规则当作复用 Core。当前 CLI/MCP 同库、租约与备份契约无法直接沿用，仅编译到 WASM 不能解决共享数据访问，因此本轮不优先该路线；未来评估仍须保持同一业务实现并证明授权外部 AI 的数据可达性。
- 复用面：前端只允许经 `api.ts` 适配器边界访问网络 API；生产路径必须移除对 `@tauri-apps/api` invoke/dialog 的必需导入（按能力探测降级），桌面保留 Tauri invoke 分支。
- 依赖基线声明为候选（HTTP 栈、SQLite/图片/keyring 原生依赖在各平台的部署证据）与验证门分离；未验证前不宣称任何"已支持的浏览器/库版本"。

### 安全设计（全部为提案，逐条待测试门）

- **绑定不是鉴权**：loopback 绑定只限制网络位置。一切普通浏览器/API 调用必须走既有 `execute_as_client`（成员 + operation scope + 到期/撤销），无匿名全量可信访问。
- **首次引导**：服务首启生成一次性引导令牌（0600 写入数据目录本机文件并向本机操作员显示），操作员在本地管理页用其**恰好一次**调用服务内可信委托（服务进程本身即可信本机进程，等价 GUI 角色）创建首个常规 client grant——无循环授权。引导令牌用后作废，可重置（重新生成本机文件 + 显式本地确认）。
- **会话**：浏览器以 grant token 换取服务会话 Cookie（HttpOnly、SameSite=Strict、服务生命周期、可单独撤销）；grant token 本身不进 Cookie/URL/构建产物/日志/明文配置；既有令牌在到期/撤销前可复用，不虚构一次性语义。登出/撤销立即影响后续请求（ADR0007 既有语义）。
- **Host/Origin/DNS rebinding**：严格 Host 白名单（仅配置的 `127.0.0.1:port`/`localhost:port`），其余 Host 一律拒绝——防 DNS rebinding。Origin 策略：状态变更请求要求同源 Origin 精确匹配；跨源、null、缺失 Origin 的非安全方法一律拒绝；不提供跨源 CORS。CSRF 由 Host/Origin 校验 + SameSite Cookie + 会话绑定的变更确认令牌共同承担，三者都有负向测试。
- **特权本地代理（trusted-local broker）**：`client.*`、`provider.credential.*`、`encryption.*`、`schedule.runtime.configure` **永不进入通用会话 API**。服务进程内实现有界本地管理代理：仅限显式本地操作员在引导/管理页执行的精确动作白名单（登记/撤销客户端、凭据设置、加密管理、调度配置），每次需本地在场确认；不复制通用 `--trusted-local` HTTP 桥。无 Tauri 依赖。
- **媒体**：上传仅浏览器→loopback，先入有界安全临时文件（大小/类型/遍历校验、崩溃清理），再调既有 `assets.import`/`imports.create`，成员 scope 不变；普通 HTTP 永不接受不可信浏览器提供的任意本机服务器路径。预览仅经既有 `assets.preview` 按成员授权；不提供任意静态文件导出。原图/中间图/试穿私有本地，仅采纳后的压缩商品展示图参与同步（无新上传默认）。
- **任务/备份/平台**：worker 沿用持久 job/回执/租约，不自造进程成功假象；如实声明服务关闭/系统休眠期间任务暂停的限制。备份/恢复/数据迁移需本机适配器 + 全成员授权 + 独占写 + 预备份（ADR0022 既有边界），不接受浏览器任意路径。凭据仍走 OS keyring（经本地代理），各平台需真实部署证据。
- **CLI/MCP 不变**：继续直接打开同一数据库的已授权契约；外部 AI 对 Web 数据的操作与 #33 契约一致，不绕过成员/scope/schema/幂等。

## 三、未决验证门（不得当作已支持）

- 候选 HTTP 栈与各平台原生依赖（SQLite、图片编解码、keyring）在 macOS/Windows/Linux 的真实打包/部署证据。
- 各 OS 支持浏览器的最低版本矩阵、Cookie/SameSite 行为、休眠/关闭恢复行为实测。
- 引导流、会话撤销传播、Origin/Host/CSRF/DNS-rebinding 负向用例、上传遍历/类型/大小攻击用例的实际红绿证据。
- 服务生命周期（休眠/关闭）下 worker 与备份的如实限制文档与用户可见提示。
- F01–F30 的 Web 逐项验收与 320–768px/触摸/键盘/离线状态证据（issue39 验收标准）。
- #25 调度延后、#27 同步在 Web 之后：均非本架构前置；已实现的 schedule/sync 能力按现状声明（存在/部分/未验证），不删除也不宣称缺席。
