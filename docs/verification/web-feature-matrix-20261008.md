# F01–F30 Web 覆盖矩阵 · 终态（2026-10-08，#39 终验轮）

本文件替代 2026-10-07 切片版矩阵。口径改为四档，**禁止以桌面结果替代 Web 验收**：

1. **浏览器实测** = 本轮经 Chrome e2e（`apps/desktop/tests-web/`，生产构建 + 真实 wardrobe-web 服务 + 真实 SQLite，23 项全绿；Chromium 对照 23 项全绿）或 Web 栈行为测试（`crates/wardrobe-web/tests/web_stack.rs`，17 项全绿）验证；
2. **通道可用 · 未逐项验收** = 与桌面共用同一 Application API 与四工作区 UI，传输层经 ADR0029 适配层等价，但浏览器逐项端到端验收未做——**不视为 Web 验收完成**；
3. **未实现（浏览器限制）** = 平台能力在浏览器不可得，界面如实标注而非伪称等价（ADR0029 D6/D7）；
4. **延后** = Windows/Linux 浏览器矩阵等后续交付门（用户 2026-10-07 已决定 Windows 延后；ADR0029 D9）。

证据日志：`/Users/links/.codex/wardrobe-execution/results/continue-20261007/ticket39-final/`（e2e 全量日志 `e2e-web-full-run8.log`（Chrome 23/23）与 `e2e-web-chromium-smoke.log`（Chromium 23/23）、行为测试 `web-stack-tests.log`（17/17）、部署演练 `deploy-rehearsal.log`、桌面回归 `desktop-regression-stylist-sync.log`、`cargo-check.log`）。模型依赖功能（识别/商品图/搭配图/试穿/Stylist）在无凭据时的**能力如实声明**已作为验收项实测：入口给出明确提示，不算失败，也不算通过真实功能。

| 功能 | Web 状态 | 说明与证据 |
|---|---|---|
| F01 多成员衣橱 | **浏览器实测** | e2e：首启引导建授权（00-bootstrap）、创建成员/切换/成员隔离（10-core）；行为测试：受限成员授权跨成员 member_denied、member.list 过滤（web_stack s2） |
| F02 单品录入 | **浏览器实测** | e2e：单件真实 PNG 上传→压缩→展示→入库（10-core）；四件真实照片录入（20-inventory）；批量 2 图→处理→任务收据→入库（10-core）；重载恢复。Web 用文件选择完成关键操作，不依赖拖拽 |
| F03 单品属性 | 通道可用 · 部分实测 | 名称/类别/收藏/季节在 e2e 中经真实表单保存并重载读回（20-inventory、40-outfits）；全字段往返沿桌面用例，Web 逐字段验收未做 |
| F04 单品维护 | **浏览器实测** | e2e：搜索、排序切换、归档→筛选→取消归档、移入回收站→恢复、全程重载持久（20-inventory test1/test2） |
| F05 AI 识别 | 通道可用 · 能力声明实测 | 识别入口在无视觉模型时如实显示"请在模型设置添加视觉模型。"并保留手工填写（30-import test2，注记 capability）；真实云识别调用未发生（需供应商凭据） |
| F06 本地图片处理 | **浏览器实测（核心链路）** | 行为测试：真实 PNG→EXIF/压缩→WebP 展示图→preview 授权字节（web_stack s4）；e2e：旋转生成预览→检查→放弃（原图保留）、坏类型 400 拒绝且无残留（30-import test1/test3）。本地抠图（U²-NetP 下载）未在浏览器逐项：入口可见、状态读取实测 |
| F07 商品展示图 | 通道可用 · 能力声明实测 | ProductDisplay 入口与"请在模型设置添加专用 Seedream 图片配置并安全保存凭据。"声明经 e2e 界面快照证实（30-import）；真实生成未发生 |
| F08 手工搭配 | **浏览器实测** | e2e：加入搭配→保存（10-core）；保存→改名→克隆→收藏→本地拼贴→回收站→恢复→重载持久全周期（40-outfits test1） |
| F09 穿搭推荐 | **浏览器实测（离线规则）** | e2e：离线规则推荐返回真实组合并保存为搭配（10-core）；固定硬约束/换一套/保存（50-care test2）；天气/AI 推荐未实测 |
| F10 推荐控制 | 通道可用 · 部分实测 | 固定（fixed_ids）硬约束浏览器实测（50-care test2）；排除/优先/替换单件经同一 UI/API，未逐项验收 |
| F11 推荐反馈 | 通道可用 · 未逐项验收 | 反馈组件经同一 API；Web 验收未做 |
| F12 实际穿着历史 | **浏览器实测** | e2e：显式记录（含昨日补记）、更正（旧事件保留+修订历史）、撤销（原因必填、计数回退、显示已撤销）、保存搭配/推荐**不隐式产生穿着**断言（10-core、40-outfits test2） |
| F13 洞察 | **浏览器实测** | e2e：统计指标、类别/颜色分布、实际穿着频率、久未穿阈值保存与恢复默认、重载重算（40-outfits test2、50-care test3） |
| F14 定时推荐 | **未实现（浏览器限制）** | #25 已在桌面端交付；浏览器无系统通知/后台定时，Web 不推进定时任务、不伪称等价（ADR0029 D7） |
| F15 搭配图 | 通道可用 · 部分实测 | 本地拼贴浏览器实测（40-outfits test1，采用保存版本商品图）；AI Flatlay 入口能力声明实测（"先在模型设置保存专用图片模型与系统安全凭据。"）；真实生成未发生 |
| F16 虚拟试穿 | 通道可用 · 部分实测 | **人物导入通道浏览器实测**：网页选择图片→/api/upload purpose=person→tryon.person.import→"人物图片已私有保存到本机"（40-outfits test1）；生成入口能力声明实测；真实试穿未发生（需 Seedream 凭据） |
| F17 Stylist Agent | 能力声明实测 | 无文本模型时如实显示"还没有可用的文本模型。先到「设置 → 模型与连接」添加…"（60-settings test4）；真实调用未发生 |
| F18 偏好学习 | 通道可用 · 部分实测 | 显式偏好（颜色/权重）保存与重载持久浏览器实测（60-settings test2）；推断学习/重算通道可用未逐项 |
| F19 CLI | **浏览器实测（一致性）** | 行为测试：Web 会话写入与 CLI 同库读回一致（web_stack s2）；部署演练：浏览器录入的单品经 bundle 内 CLI 读回一致（deploy-rehearsal 步骤 4/6） |
| F20 MCP | 不变（本机入口） | 与 F19 同一注册源与授权模型；MCP stdio 不经浏览器（ADR0029 D5） |
| F21 同步 | 延后（如实标注） | sync.* 经同一 api() 适配层进入 Web 构建；本地文件夹选择依赖桌面对话框、凭据只写系统安全存储（BROWSER_LIMITS.folderPicker/credentials 如实标注）。浏览器端到端验收延后；本轮未改动 SyncSettings.tsx |
| F22 备份与迁移 | **浏览器实测（创建+预览）** | 行为测试：浏览器会话 backup.create → maintenance.worker.run 收据 succeeded → backup.preview 清单（web_stack s5）；e2e：设置页创建本地备份→"备份已创建，所有列出的文件已校验"→预览恢复范围与清单计数（60-settings test3）。恢复执行沿 ADR0022 核心验收，浏览器端不重复执行覆盖恢复 |
| F23 软件设置 | **浏览器实测** | e2e：模型连接保存+重载持久、凭据边界如实标注（provider.credential.* 属仅可信入口，状态读取亦被拒，页面不提供写密钥按钮，60-settings test1）；穿衣偏好（test2）；备份（test3）；客户端授权/加密的浏览器拒绝（web_stack s3、10-core note 断言） |
| F24 跨平台发行 | 延后（如实标注） | 本轮 macOS Chrome 154 全套 23 项 e2e 全绿（交付基线）+ Playwright Chromium 全套 23 项对照全绿；Safari/Firefox 本机无对应驱动/环境，**未测**；Windows/Linux 浏览器矩阵延后（用户已决定 Windows 推迟；ADR0029 D9） |
| F25 洗护 | **浏览器实测** | e2e：阈值配置（乐观锁）、手动标记待洗→待洗列表、干洗解除→可推荐、更正干洗日期→待洗重算生效、撤销→修订历史 3、重载持久（50-care test1） |
| F26 多图维护 | 通道可用 · 部分实测 | e2e：批量导入→逐张采用、主图切换（主图标记跟随图片）、排序、旋转预览放弃、坏类型拒绝（30-import test1/test3）；拼图 ImageMerge、本地抠图、替换主图通道可用未逐项 |
| F27 Lookbook / Studio | **浏览器实测（核心）** | e2e：穿搭库全周期（40-outfits test1）；"今天穿这套"模板穿着、场景计划、购买分析通道可用未逐项 |
| F28 单品配对 | 通道可用 · 未逐项验收 | PairingStudio 经同一 API/UI；Web 验收未做 |
| F29 外部 Agent 模式 | **浏览器实测（边界）** | 行为测试+e2e：特权操作 scope_denied、撤销即时传播（authorization_revoked 踢回登录）、/auth/state 不泄露令牌、CSRF/Origin/Host 负向、CLI 同库读回一致（web_stack s3、99-security） |
| F30 基线细节 | 通道可用 · 部分实测 | 收藏筛选（20-inventory）、键盘可达 skip-link（10-core/20/60）、375px 无横向溢出（20/60）、375/768 内布局沿用桌面四工作区重设计；评分/当天排除/Try Another 通道可用未逐项 |

## 浏览器与安全专项（本轮实测）

- 引导：一次性令牌文件（0600）→ 恰好一次 → 令牌文件消费后不可复用（e2e + 部署演练）。
- 会话：HttpOnly+SameSite=Strict Cookie 属性断言；服务内存会话；登出/撤销即时生效；连接令牌不出现在 Cookie/页面状态/日志；服务重启后须连接令牌重登录（部署演练 resume 阶段实证）。
- CSRF 三层负向：伪造 Host 403、缺失/跨源 Origin 403、缺失/错误 CSRF 头 403（行为测试+真实浏览器 e2e）。
- 上传边界：20MiB/64MiB/100 文件/类型白名单；拒绝不落残留；任意服务器路径不可由浏览器指定（行为测试 + e2e 坏类型负向）。
- 能力如实声明：识别/Flatlay/Tryon/Stylist/凭据/加密/客户端授权在无凭据或浏览器不可得时给出明确文案，不提供伪造入口（e2e 注记 capability）。
- 窄屏与键盘：375px 无页面级横向溢出（库存/设置）；skip-link 键盘可达并聚焦主内容。

## 部署形态验收（本轮实测，详见 docs/api/web.md 第四节）

- `scripts/web/pack-web-release.sh` 打包 v1（0.1.0，bin sha256 66bf0874…）→ `start-web.sh` 启动（127.0.0.1:5392，数据目录独立）→ 真实 Chrome 引导+录入+重载恢复 → CLI 同库读回一致 → 停止 → 打包 v2 → `upgrade-web.sh` 升级重启 → 连接令牌重登数据完好 → `--rollback` 回滚 v1 再次验证 → 升级回 v2 → 停止（不留常驻进程）。
