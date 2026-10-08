# Web 端运行与交付（wardrobe-web）

状态：2026-10-08，#39 终验轮交付。架构决策见 [ADR0029](../adr/0029-local-web-service.md)；F01–F30 在 Web 的逐项四档状态（浏览器实测 / 通道可用未逐项 / 未实现-浏览器限制 / 延后）见 [web-feature-matrix-20261008.md](../verification/web-feature-matrix-20261008.md)。本文是运行、构建、测试、部署/升级/回滚与已知限制的入口。

## 一、运行

- 启动：`wardrobe-web [--port <端口>] [--data-dir <目录>] [--web-dist <界面目录>]`（或环境变量 `WARDROBE_DATA_DIR` / `WARDROBE_WEB_DIST`）。
- 访问：`http://127.0.0.1:17838/`（默认端口 17838；服务只绑定 127.0.0.1，不监听公网）。
- 数据位置：与桌面/CLI 完全一致（`default_database_path()`；`WARDROBE_DATA_DIR` 覆盖；`--data-dir` 显式指定）。浏览器内所见即本机 SQLite 数据；原图/中间图/真人/试穿/AI 搭配图保持本机。
- 首次使用：服务在数据目录写一次性引导令牌文件 `web-bootstrap-token`（0600）。本机操作员在网页首启页粘贴其内容，服务端恰好一次创建浏览器客户端授权并返回一次性连接令牌（请保存到密码管理器）；之后用连接令牌登录。
- 会话：Cookie `wardrobe_session`（HttpOnly、SameSite=Strict），只存在于服务进程内存——**服务重启后需要用连接令牌重新登录**（部署演练 resume 阶段实证）。在设置页撤销该客户端授权会立即把所有浏览器会话踢回登录页。

## 二、构建（可复现）

```sh
. /Users/links/.codex/wardrobe-execution/native-env.sh   # 构建须在外盘工作树内执行（libheif 等原生依赖）
export CARGO_TARGET_DIR=/Users/links/.codex/wardrobe-execution/cache/ticket39-final
cargo build -p wardrobe-web --release        # 服务 release 二进制（含 wardrobe-core 一起编译）
cd apps/desktop && npm ci && npm run build   # 前端产物在 apps/desktop/dist
sh scripts/web/pack-web-release.sh --worktree <worktree> --out <备份盘打包根> \
  --bin $CARGO_TARGET_DIR/release/wardrobe-web --cli $CARGO_TARGET_DIR/release/wardrobe-cli
```

- 打包产出 `releases/v<版本>-<时间戳>/`（`bin/`、`web-dist/`、`lib/` 原生媒体动态库、`scripts/`、`VERSION` 含来源 commit 与 bin sha256），并切换 `<out>/current` 软链；升级与回滚只动这个软链。
- 依赖缓存：cargo 与 npm 缓存均由 native-env 指向备份盘（docs/agents/build-storage.md）。
- 开发态：`npm run dev` 仍走仅回环的 Vite 开发桥（不进生产 bundle）；生产浏览器构建不依赖 Tauri 运行时（api.ts/platform.ts 适配层）。
- 本机开发直跑二进制需要 native-env 的 `DYLD_LIBRARY_PATH`（libheif/libde265）；独立分发的 bundle 自带 `lib/`，`start-web.sh` 会自动设置。

## 三、测试

```sh
# 行为测试：真实 TCP/HTTP + 真实 SQLite + 真实图片解码（17 项）
cargo test -p wardrobe-web --test web_stack
# 浏览器 e2e：生产构建 + 真实服务（本轮 23 项，见 tests-web/00–99）
cd apps/desktop && WARDROBE_WEB_BIN=<wrapper 二进制> WARDROBE_CLI_BIN=<cli> \
  WARDROBE_WEB_TEST_PORT=5391 WARDROBE_WEB_TEST_DATA=<备份盘数据目录> \
  npx playwright test --config playwright.web.config.ts
# 跨引擎冒烟（可选）：WARDROBE_WEB_CHANNEL=chromium …
```

e2e 配置接受 `WARDROBE_WEB_BIN` / `WARDROBE_CLI_BIN` / `WARDROBE_WEB_TEST_DATA` / `WARDROBE_WEB_TEST_PORT` / `WARDROBE_WEB_CHANNEL`；`tests-web/.auth/` 为运行时产物，不入库。测试文件按 `00→99` 序号串行依赖（bootstrap → 各工作区 → 安全边界），须整组运行。

## 四、部署 / 升级 / 回滚（本机已实际演练）

部署形态：本机单用户进程 + 版本化目录 + `current` 软链；数据目录即用户数据；无安装器、无服务注册、无公网组件。脚本：`scripts/web/pack-web-release.sh`、`start-web.sh`、`stop-web.sh`、`upgrade-web.sh`、`deploy-smoke.mjs`、`rehearse-deploy.sh`。

2026-10-08 在本机按 `rehearse-deploy.sh` 完整演练（端口 5392，独立数据目录，证据 `results/continue-20261007/ticket39-final/deploy-rehearsal.log`）：

1. **部署**：打包 v1（0.1.0，bin sha256 `66bf0874…`，来源 commit `ded5ffe`）→ 启动 → `/health` 200 → 引导令牌文件权限 `-rw-------`。
2. **浏览器跑通核心流程**：真实 Chrome 完成 引导令牌登记 → 获得连接令牌 → 界面录入真实照片单品 → 页面重载后数据恢复。
3. **一致性**：bundle 内 wardrobe-cli `--trusted-local clothing.list` 读回同一单品，与浏览器会话写入一致。
4. **升级**：`stop-web.sh` 停止（`/health` 不可达确认）→ 打包 v2（release 二进制）→ `upgrade-web.sh` 切软链重启 → 健康检查通过。
5. **升级后验证**：连接令牌重新登录成功（会话不持久化，符合预期）；升级前录入的单品与照片完好；CLI 读回一致。
6. **回滚**：`upgrade-web.sh --rollback` 切回 v1 重启 → 浏览器重登、数据完好 → 再升级回 v2 → 停止服务（演练结束，不留常驻进程）。

- 升级语义：替换二进制与 dist 后重启即可；数据库 schema 升级由 wardrobe-core 迁移（与桌面/CLI 同一机制，跨进程串行化）。连接令牌与数据不受影响。
- 回滚边界（如实标注）：`--rollback` 只覆盖 schema 未变化的常规回退；数据库由较新版本升过级后，旧版本会以 `unsupported_schema` 拒绝打开——此时必须先恢复升级前的应用级备份（备份与恢复见设置页，沿 ADR0022：预览 → 确认覆盖 → 自动脉预备份）。演练中的升级/回滚未发生 schema 变化。
- 数据备份：与桌面一致（backup.create/preview/restore，全成员授权 + 独占写 + 预备份）；浏览器会话可直接创建与预览（行为测试 + e2e），恢复执行沿 ADR0022 核心验收。升级前可另用 `upgrade-web.sh --backup-dir` 做停机 SQLite 快照。

## 五、已知限制（如实标注，不伪称与原生等价）

1. **平台能力边界**：浏览器无系统通知、无后台/定时执行、无开机启动。任务（imports/images/learning/maintenance）沿用持久队列，但只在页面打开且服务运行时由页面驱动推进；页面关闭任务暂停，重启后可查询/重试。#25 定时推荐已在桌面端交付，Web 端不推进定时任务。
2. **可信本机能力**：`client.*`、`provider.credential.*`、`encryption.*`（及 `schedule.runtime.configure`）在浏览器会话中被核心拒绝（含 `provider.credential.status` 读取——模型页因此如实显示"凭据状态未读取"且不提供写密钥入口）；外部客户端（CLI/MCP AI）的授权登记不经浏览器。
3. **会话生命周期**：服务重启即登出（会话不持久化、令牌不落盘），须用连接令牌重登录。持久会话或 OS keyring 集成涉及数据边界，待用户决策（ADR0029 验证门）。
4. **网络位置**：仅 127.0.0.1。LAN/远程访问会改变数据边界，未实现、未经用户决策不做。
5. **并发**：服务内单 `Application` 互斥串行；长任务（备份/恢复）执行期间其他 API 请求排队——单用户场景可接受。
6. **浏览器矩阵（本轮实测范围）**：macOS Chrome 154 全套 23 项 e2e 全绿（交付基线）；Playwright Chromium 全套 23 项对照全绿（`WARDROBE_WEB_CHANNEL=chromium`，同一 Blink 引擎族）。**Safari 与 Firefox 未测**（本机无对应驱动/测试环境，如实标注，未以任何结果替代）；声明的最低基线（Chrome/Edge ≥120、Firefox ≥121、Safari ≥17）与 Windows/Linux 真机验证为后续交付门（ADR0029 D9；用户已将 Windows 延后）。
7. **上传限制**：单图 20 MiB、单批 64 MiB/100 文件、类型 JPEG/PNG/WebP/HEIC/HEIF，与桌面一致；超大或非法输入被拒绝且不落残留（有负向测试 + 浏览器负向 e2e）。
8. **模型依赖功能**：识别、商品展示图、AI 搭配图、虚拟试穿、Stylist 的真实云调用需要用户自行配置供应商凭据（凭据只能在桌面端或 CLI 写入系统安全存储）。无凭据时各入口给出明确提示并保留手工/离线路径，浏览器端不伪造结果。
9. **桌面侧栏裁剪**：1280×900 下侧栏"管理成员档案"表单被 100dvh 侧栏裁剪（既有桌面问题，#34 范畴）；Web e2e 以高窗口完成成员流程并如实记录。
