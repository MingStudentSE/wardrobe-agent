# ADR0029 · 本机 Web 服务架构

状态：已接受（2026-10-07），#39；取代同日草案（8f1deae）。本文由草案定稿为可执行决策，随实现更新验证门状态，不把未验证项当作已支持。执行顺序依用户决定：Web 现在，#25 调度延后，#27 同步在 Web 之后；macOS 桌面 Beta 保留，Windows/Linux 桌面包延后。

本文严格区分三部分：**观察到的现状**（引用实际源码）、**已接受的决策**（可执行，含本轮实现边界）、**验证门**（未通过的项必须如实标注为未支持）。

## 一、观察到的现状（源码事实，定稿时复核）

- 共享边界：`crates/wardrobe-core/src/lib.rs` 的 `Application::execute` / `execute_as_client` 是唯一公开命令/查询边界（ADR0001）；`default_database_path()` 提供三平台一致数据目录与 `WARDROBE_DATA_DIR` 覆盖；bundled SQLite + WAL + IMMEDIATE 迁移事务。
- 外部授权：`crates/wardrobe-core/src/authorization.rs` 的 `execute_as_client` 先查 token hash、撤销/到期、精确 operation scope、成员范围，再入业务；`privileged()` 将 `client.*`、`provider.credential.*`、`encryption.*` 限定可信本机（另有精确名特权操作 `schedule.runtime.configure`）。令牌仅在登记时返回一次，库内只存 SHA256，复用语义为到期/撤销前有效（ADR0007）。
- CLI/MCP：`crates/wardrobe-cli/src/main.rs` 直接打开同一数据库，外部模式经 `--client-stdin`（ADR0007）；registry 元数据（scope/effect/examples）由 `operations()` 单源派生。
- 桌面桥现状：`apps/desktop/src/api.ts` 为 Tauri `invoke('application')`；DEV 且非 Tauri 环境 fetch `/__wardrobe_dev_api`。`apps/desktop/vite.config.js` 的三个 `/__wardrobe_dev_*` 中间件是开发专用桥（每命令 spawn `wardrobe-cli --trusted-local`、无会话/CSRF/DNS-rebinding 防护），不进生产 bundle；生产 Tauri 路径依赖 Tauri 运行时。两者都不能当作跨平台 Web 交付。

## 二、已接受的决策

### D1 路径选择：独立同源本机回环服务

采用**独立本机 Rust 服务 `wardrobe-web`**：直接链接 `wardrobe-core`，进程内持有一个 `Application`（Mutex 串行化），同源服务静态 React UI 与 JSON API；只绑定 `127.0.0.1`。无公网后端、无账号体系、无远程监听默认、无照片上传互联网默认。否决两条备选：

- 浏览器本地运行（WASM/OPFS/IndexedDB）：浏览器源内存储是浏览器沙箱，当前 Rust SQLite/CLI/MCP 无法打开；走此路需要验证 Core 的 WASM 可移植性，并新增存储/媒体/凭据/worker 适配；把 IndexedDB 中另写库存/统计/推荐规则视为"复用 Core"不可接受。本轮不采用，未来评估仍须同一业务实现且外部 AI 数据可达。
- 沿用桌面开发桥或 Tauri 壳：开发桥按命令 spawn 子进程、全量 trusted-local、无会话语义，不能承载生产；Tauri 壳要求先装桌面客户端，违背"三 OS 浏览器可用"的验收。两者都只保留原角色（开发桥仅 DEV，Tauri 仅桌面壳）。

不改变既有本地优先与数据边界：原图/中间图/真人/试穿/AI 搭配图保持本机；仅采纳后的压缩商品展示图具备同步资格；若未来需要 LAN 访问、远程部署或持久会话等改变数据边界的方案，必须先单独记录并取得用户决策。

### D2 HTTP 栈：tiny_http（同步、最小依赖面）

选定 `tiny_http 0.12` 而非 axum/hyper：本服务是单用户回环服务，阻塞 SQLite 调用经 Mutex 串行化即可正确工作，异步栈的吞吐优势无意义，而 axum 引入 100+ 传递依赖（供应链与审计面）。tiny_http 依赖面极小、同步模型与安全检查（Host/Origin/Cookie 头逐条判断）一一对应，便于负向测试。HTTP 解析风险由成熟库承担；本仓库不手写 HTTP 解析。

### D3 进程、端口与启动

- 启动：`wardrobe-web`（workspace 新 crate，二进制 `wardrobe-web`）。默认监听 `127.0.0.1:17838`，`--port` 可改端口；**不接受非回环绑定**（无 `--host` 参数；未来如需 LAN 必须先走用户决策）。
- 访问地址：`http://127.0.0.1:17838/`（或 `http://localhost:17838/`）。
- 数据位置：默认与桌面/CLI 完全一致（`default_database_path()`；`WARDROBE_DATA_DIR` 覆盖）；`--data-dir` 显式指定。Web 不引入第二套数据目录规则；浏览器内所见数据即本机数据库数据。
- 静态资源：`--web-dist <目录>`（或 `WARDROBE_WEB_DIST`）指定 React 构建产物目录；缺省时回退到可执行文件旁 `web-dist`。目录缺失时服务仍提供 API 并在 `/` 返回提示页（不当作 UI 交付）。
- 上传临时目录：数据目录下 `web-uploads-tmp/`；启动时清理崩溃残留；临时文件随请求结束删除。构建、缓存、临时文件位于备份盘由调用方以 `WARDROBE_DATA_DIR` 指向备份盘实现（遵循 docs/agents/build-storage.md）。

### D4 鉴权与会话（复用 client authorize 模型，不另造权限）

- 一切业务 API 只经 `Application::execute_as_client`（成员 + operation scope + 到期/撤销，每次调用实时校验；撤销立即传播）。服务进程对普通浏览器请求**不是** trusted-local：不存在匿名全量可信通道。回环绑定只是网络位置限制，不是鉴权。
- 首次引导：数据目录无任何 client grant 时，服务在数据目录写一次性引导令牌文件 `web-bootstrap-token`（0600，32 字节随机 hex），并在启动日志与 `/` 提示页告知本机操作员路径。操作员（本机在场）提交该令牌调用 `POST /auth/bootstrap` 恰好一次：服务端以可信本机身份执行 `client.authorize`（名称"Web 浏览器"，full 模式），随后删除令牌文件。响应一次性返回连接令牌，供其他浏览器登录使用；令牌不落服务端日志、URL、Cookie 或普通明文配置。
- 登录：`POST /auth/login {token}` 用连接令牌换取会话 Cookie（`wardrobe_session`，HttpOnly、SameSite=Strict、Path=/）。会话保存在服务进程内存（含会话 ID、csrf、grant token），**不持久化**：服务重启后会话失效，须用连接令牌重新登录（与 CLI 令牌保管方式一致；持久会话或 OS keyring 集成留待用户决策）。浏览器 JS 永远拿不到 grant token。
- CSRF 三层：Host 白名单（仅 `127.0.0.1:port`/`localhost:port`，其余一律 403，防 DNS rebinding）+ 非 GET 请求 Origin 必须存在且与 Host 精确同源（缺失/跨源/null 一律 403）+ 会话绑定 CSRF 令牌（登录后 `GET /auth/state` 返回，变更请求须带 `X-Wardrobe-CSRF`）。Cookie 另有 SameSite=Strict。每层都有负向测试。
- 特权操作（`client.*`、`provider.credential.*`、`encryption.*`、`schedule.runtime.configure`）永不进入会话 API——`execute_as_client` 本身拒绝，服务不再开设旁路。浏览器设置页如实标注："客户端授权/模型凭据/加密空间须在桌面端或 CLI（可信本机入口）管理"。浏览器自身的 grant 由服务端引导流程创建。

### D5 媒体边界（与桌面完全一致）

- 上传仅浏览器→回环：`POST /api/upload`（单图，原始字节，`?member=` 指定成员）与 `POST /api/batch-upload`（批量 JSON）。限制与桌面一致：单文件 20 MiB、单批 64 MiB、最多 100 文件、类型限 JPEG/PNG/WebP/HEIC/HEIF；文件名只取 basename，服务端自建临时路径，绝不接受浏览器提供的任意服务器路径。
- 落盘后调用既有 `assets.import` / `imports.create`（成员 scope 不变，校验/压缩/入库逻辑全部复用核心），随后删除临时文件。崩溃残留由启动清理兜底。
- 预览仅经既有 `assets.preview`（成员授权，data_url 返回），不提供任意静态文件导出。原图、缩略图、中间图、生成图保持本机；同步白名单边界不变（ADR0004/0013/0016）。

### D6 UI 复用与平台能力适配层

- 生产 Web 与桌面共用同一 React 四工作区构建（apps/desktop），不复制页面或视觉系统。传输差异收敛在 `api.ts`：Tauri → `invoke('application')`；DEV 非 Tauri → 开发桥（不变）；生产浏览器 → Web 会话 API。生产浏览器构建不得把 Tauri API 当作必需导入。
- 平台能力（文件/目录选择、安全凭据、系统通知、后台定时）经适配层声明：浏览器以 `<input type="file">` 提供文件选择；目录选择、任意服务器路径浏览、系统通知、后台执行在 Web 不提供，UI 明确显示"浏览器不可用/需桌面端"，不伪称与原生等价。城市搜索（LocationDialog）沿用既有浏览器直连 open-meteo 分支（无凭据公开接口）。

### D7 任务、备份与平台能力

- 任务沿用持久 job/回执/租约与 jobs.* / maintenance.* API；本轮 worker 仍由页面显式驱动（与桌面一致的 `imports.process`、`maintenance.worker.run`、`images.worker.run`、`learning.worker.run` 调用）。**服务不为 Web 常驻后台 worker**：页面关闭或服务停止时任务暂停，重启后可查询/重试（持久队列），UI 如实标注"浏览器关闭后任务不推进"。浏览器推送/系统通知不提供（留待用户决策的适配器工作）。
- 备份/恢复/数据目录迁移沿用 ADR0022：经 maintenance API + 全成员授权 + 独占写 + 预备份；路径为本机绝对路径文本输入（服务在本机运行，操作员即本机用户），核心负责校验；不接受浏览器任意文件导出。

### D8 CLI/MCP 契约一致性

Web 不新增能力注册源：`capabilities` 由同一 `operations()` 单源派生；外部 AI 经 CLI/MCP 直接操作同一数据库，成员/scope/schema/幂等校验不被 Web 绕过。浏览器会话自身就是一个外部 client grant（full 模式、非特权），其可调用集合与 CLI full 授权一致（特权操作除外）。浏览器数据被授权外部 AI 操作的方式与桌面数据完全相同。

### D9 支持浏览器基线（待逐项实测确认为交付门）

声明基线：Chrome/Edge ≥ 120、Firefox ≥ 121、Safari ≥ 17（依赖 ESM、File/FormData、fetch、SameSite Cookie、WebP 显示）。低于基线不承诺。**本轮已实测**：macOS Chromium（Playwright）与系统 Chrome 关键流程；Windows/Linux 浏览器实测延后并在交付文档如实标注。320/375/414/768px、键盘、触摸与既有桌面验收同标准。

## 三、本轮实现范围与验证门（2026-10-07 切片）

已实现并有红绿证据（见交付报告与 results 目录）：S0 服务启动/静态/健康、S1 引导与会话、S2/S3 会话 API 写入与 CLI 回读、CSRF/Origin/Host 负向、S4 媒体上传/导入/预览与超限拒绝、S5 特权操作拒绝的浏览器呈现、核心四工作区流程（成员、单品录入含真实照片、手工搭配、穿着/洗护、洞察、离线推荐、设置读取）的 Playwright e2e 与行为测试。

仍为验证门（不当作已支持）：

- S6 常驻 worker、系统通知、后台定时（如实标注浏览器限制；#25 延后）。
- 三 OS 真机浏览器矩阵与逐项最低版本实测（本轮仅 macOS Chromium/Chrome）。
- 原生依赖在 Windows/Linux 的打包部署证据；安装产物与升级/回滚仅在文档层给出（可复现构建以源码+脚本为准）。
- 持久会话/OS keyring 集成、LAN 访问：涉及数据边界，待用户决策。
