# Web 端运行与交付（wardrobe-web）

状态：2026-10-07，#39 本轮切片交付。架构决策见 [ADR0026](../adr/0026-local-web-service.md)；F01–F30 在 Web 的逐项状态见 [web-feature-matrix-20261007.md](../verification/web-feature-matrix-20261007.md)。本文是运行、构建、测试、部署/升级/回滚与已知限制的入口。

## 一、运行

- 启动：`wardrobe-web [--port <端口>] [--data-dir <目录>] [--web-dist <界面目录>]`（或环境变量 `WARDROBE_DATA_DIR` / `WARDROBE_WEB_DIST`）。
- 访问：`http://127.0.0.1:17838/`（默认端口 17838；服务只绑定 127.0.0.1，不监听公网）。
- 数据位置：与桌面/CLI 完全一致（`default_database_path()`；`WARDROBE_DATA_DIR` 覆盖；`--data-dir` 显式指定）。浏览器内所见即本机 SQLite 数据；原图/中间图/真人/试穿/AI 搭配图保持本机。
- 首次使用：服务在数据目录写一次性引导令牌文件 `web-bootstrap-token`（0600）。本机操作员在网页首启页粘贴其内容，服务端恰好一次创建浏览器客户端授权并返回一次性连接令牌（请保存到密码管理器）；之后用连接令牌登录。
- 会话：Cookie `wardrobe_session`（HttpOnly、SameSite=Strict），只存在于服务进程内存——**服务重启后需要用连接令牌重新登录**。在设置页撤销该客户端授权会立即把所有浏览器会话踢回登录页。

## 二、构建（可复现）

```sh
. /Users/links/.codex/wardrobe-execution/native-env.sh   # 构建须在外盘工作树内执行（libheif 等原生依赖）
export CARGO_TARGET_DIR=/Users/links/.codex/wardrobe-execution/cache/ticket39-web
cargo build -p wardrobe-web           # 服务（含 wardrobe-core 一起编译）
cd apps/desktop && npm ci && npm run build   # 产物在 apps/desktop/dist
```

- 依赖缓存：cargo 与 npm 缓存均由 native-env 指向备份盘（docs/agents/build-storage.md）。
- 开发态：`npm run dev` 仍走仅回环的 Vite 开发桥（不进生产 bundle）；生产浏览器构建不得依赖 Tauri 运行时（api.ts/platform.ts 适配层）。
- 注意：开发机直跑二进制需要 native-env 的 `DYLD_LIBRARY_PATH`（libheif/libde265）；脱离构建环境的独立分发需静态处理这些库（与 CLI 安装同一边界，见 docs/api/cli-install-macos.md）。

## 三、测试

```sh
# 行为测试：真实 TCP/HTTP + 真实 SQLite + 真实图片解码（17 项）
cargo test -p wardrobe-web --test web_stack
# 浏览器 e2e：生产构建 + 真实服务（6 项；需要 Chrome 与已构建 dist/二进制）
cd apps/desktop && npm run test:e2e:web
# 桌面既有套件回归
cd apps/desktop && npm run test:e2e
```

e2e 配置 `playwright.web.config.ts` 用 `WARDROBE_WEB_BIN` / `WARDROBE_CLI_BIN` / `WARDROBE_WEB_TEST_DATA` / `WARDROBE_WEB_TEST_PORT` 指定二进制与数据目录；`tests-web/.auth/` 为运行时产物，不入库。

## 四、部署 / 升级 / 回滚

- 部署形态：本机单用户进程（源码构建或后续打包），数据目录即用户数据；无安装器、无服务注册、无公网组件。
- 升级：替换 wardrobe-web 二进制与 dist 后重启即可；数据库 schema 升级由 wardrobe-core 迁移（与桌面/CLI 同一机制，跨进程串行化）。会话不持久化，升级重启后重新登录；连接令牌与数据不受影响。
- 回滚：换回旧二进制即可；数据库由较新版本升过级时，旧版本会拒绝打开（unsupported_schema）——先恢复升级前备份（备份与恢复见设置页，沿 ADR0022：预览 → 确认覆盖 → 自动脉预备份）。
- 数据备份：与桌面一致（backup.create/preview/restore，全成员授权 + 独占写 + 预备份）；浏览器会话可直接驱动（有行为测试），原图是否包含由备份设置决定。

## 五、已知限制（如实标注，不伪称与原生等价）

1. **平台能力边界**：浏览器无系统通知、无后台/定时执行、无开机启动。任务（imports/images/learning/maintenance）沿用持久队列，但只在页面打开且服务运行时由页面驱动推进；页面关闭任务暂停，重启后可查询/重试。#25 定时推荐延后。
2. **可信本机能力**：`client.*`、`provider.credential.*`、`encryption.*`（及 `schedule.runtime.configure`）在浏览器会话中被核心拒绝，设置页明确标注"须在桌面端或 CLI 管理"。外部客户端（CLI/MCP AI）的授权登记不经浏览器。
3. **会话生命周期**：服务重启即登出（会话不持久化、令牌不落盘）。持久会话或 OS keyring 集成涉及数据边界，待用户决策（ADR0026 验证门）。
4. **网络位置**：仅 127.0.0.1。LAN/远程访问会改变数据边界，未实现、未经用户决策不做。
5. **并发**：服务内单 `Application` 互斥串行；长任务（备份/恢复）执行期间其他 API 请求排队——单用户场景可接受。
6. **浏览器矩阵**：本轮仅在 macOS Chromium（Playwright）实测。声明的最低基线（Chrome/Edge ≥120、Firefox ≥121、Safari ≥17）与 Windows/Linux 真机验证为后续交付门（ADR0026 D9）。
7. **上传限制**：单图 20 MiB、单批 64 MiB/100 文件、类型 JPEG/PNG/WebP/HEIC/HEIF，与桌面一致；超大或非法输入被拒绝且不落残留（有负向测试）。
8. **桌面侧栏裁剪**：1280×900 下侧栏"管理成员档案"表单被 100dvh 侧栏裁剪（既有桌面问题，#34 范畴）；Web e2e 以高窗口完成成员流程并如实记录。
