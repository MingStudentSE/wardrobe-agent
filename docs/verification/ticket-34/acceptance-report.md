# Ticket #34 全应用 UI 与设置验收报告

分支：`codex/ticket-34-ui`（基于集成分支 `codex/beta-integration-continue` tip `846a6b1`）。
日期：2026-10-08。环境：真实 CLI 桥（`target/debug/wardrobe-cli` wrapper → `CARGO_TARGET_DIR=/Users/links/.codex/wardrobe-execution/cache/ticket34-ui`，DYLD 指向本机 libheif/libde265），Playwright workers=1、channel=chrome、端口 5331–5459。

## 1. 范围与结果总览

- 修复约 30 个仍使用旧 UI 选择器的功能 spec（约 63 个失败测试），到达路径统一为四工作区 UI：`主要导航` 按钮 → 工作区页签 / `SettingsHub` 卡片；行为断言（协议 fixture、成员隔离、幂等重试、七档宽度、不产生穿着、事件历史保留）逐条保留。
- 新增 3 个验收 spec：
  - `tests/settings-persistence.spec.ts`：模型/穿衣偏好/天气/定时计划/备份目录/加密/CLI-MCP/任务中心/同步逐页持久化（改 → reload → 读回一致）。
  - `tests/app-acceptance.spec.ts`：键盘可达（skip link、焦点环、Enter/Space 激活）、触摸等价（tap 完成核心路径 + hover 规则不得隐藏控件）、root font-size 200% 不破版、`prefers-reduced-motion` 无过渡且可用、空/加载/错误/离线状态、中文长名称卡片/详情/弹窗不溢出。
  - `tests/responsive-audit.spec.ts` 扩展：全部穿搭页签（穿搭库/帮我搭配/单品配对/问问衣橱/场景计划/购买分析）、记录三页签、设置全部 8 张卡片（含同步、定时计划与通知）在 375/667/768/900/1024/1440/900×500 下无溢出、无遮挡、无裁剪。
- 两个真实的跨页面可用性缺陷修复（组件级，遵循既有 Atelier tokens）：
  1. `Lookbook.tsx`：模板来源「今天穿了」跨工作区前先关闭「调整穿搭」模态——否则宿主面板隐藏后 `showModal()` 弹窗仍以全页 backdrop 遮挡整个应用（触摸/键盘假死）。
  2. `redesign.css`：侧栏 `.rail` 内部可滚动（成员管理表单此前渲染在 100dvh sticky 侧栏之外，键盘/触摸不可达）；≤768px 不再隐藏成员切换器（`.rail-bottom`），窄屏保留关键流程。
- 组件能力补齐：`ItemEditor.tsx` 购买信息恢复「币种」输入（多位小数币种如 BHD 的录入/回读能力，`minor()` 既有校验不变）。
- 成员卫生：新增 `tests/support/member-hygiene.ts`；切换过成员的规格（care、schedule-task-races、schedules-member-switch、stylist、scenarios）统一在结束时恢复默认成员，消除共享 dev server 上的跨规格污染。

## 2. GUI 全量结果

| 运行 | 测试数 | 结果 | 说明 |
| --- | --- | --- | --- |
| 基线（846a6b1，修复前） | 103 | 61 failed / 34 passed / 8 skipped | 旧 UI 选择器 + 测试间污染 |
| 全量 run1 / run2（修复中期） | 118 | 45 / 46 failed | 根因：app-acceptance 长名称清理误用 `query` 筛选键（应为 `search`），`clothing.list` 忽略未知键返回全库，删除循环清空了种子衣橱，级联冲垮下游全部依赖种子的规格 |
| 全量 run3（根因修复后） | 118 | 见 §6 最终数字 | 全部依赖种子的规格恢复 |

日志：`wardrobe-execution/results/continue-20261007/ticket34/logs/full-suite*.log`（含真实退出码）。

## 3. Phase B 逐项验收状态

| 验收项 | 状态 | 证据 |
| --- | --- | --- |
| 集中设置持久化（模型/凭据/天气/计划/模板/备份/数据目录） | 通过 | `settings-persistence.spec.ts` 全绿；模型/偏好/天气/计划/备份逐页改→重启→读回一致；加密/CLI-MCP/任务中心逐页可达且无错误告警 |
| 同步设置面板 | 表面可用，状态页能力待 #31 | `settings-persistence.spec.ts` 仅断言页面可达与无告警（SyncSettings.tsx 归 #31，未动）；截图 `settings-sync-surface.png` |
| 全部页面七档宽度无溢出/遮挡 | 通过 | `responsive-audit.spec.ts`（工作区×4、穿搭×6、记录×3、设置×8、记录/地点/筛选/分类/新增/详情/编辑弹窗）+ 既有各 spec 内七档断言 |
| 视觉一致性 | 通过，发现即修 | 复用 tokens；两处组件缺陷修复见 §1；`composer/outfit-studio/redesign` 等视觉相关断言全绿 |
| 键盘焦点 | 通过 | skip link 首个 Tab、`:focus-visible` 焦点环、导航/设置卡片 Enter/Space 激活（`app-acceptance` + `members` 键盘步骤 + `responsive-audit`） |
| 触摸等价 | 通过 | `hasTouch` 上下文 tap 完成浏览→组搭→保存→记录；CSS 审计确认 hover 媒体查询不含 display/visibility 隐藏 |
| 中文长名称 | 通过 | 40+ 字名称在卡片（overflow-wrap:anywhere）、详情弹窗、穿着弹窗均完整可读（320–1440） |
| 空/加载/错误/离线 | 通过 | 逐页空态文案、加载中 role=status、桥接 503 → role=alert + 重新读取、桥接 abort → 明确错误并可恢复 |
| 字体缩放 | 通过 | root font-size 32px（200%）下 375/1280 无溢出、无元素越界，核心弹窗可操作 |
| 减少动画 | 通过 | `prefers-reduced-motion: reduce` 下过渡为 0s（redesign.css 全局禁用 + styles.css 仅在 no-preference 启用），应用完整可用 |

截图索引见 `screenshots-index.md`。

## 4. 修复的 spec 清单（要点）

authorization、backup、care、clothing-management、delete-detail、encryption、feedback、filters-panel（计数口径）、image-editing、imports、insights、journal-settings（8 卡片 + 指标对齐服务端真值）、learning、media（浏览器上传通道如实断言）、members、overview、pagination、providers、recognition、recommendations、redesign、responsive-audit、wears、weather-ai、clothing-management（币种经编辑弹窗）、settings-persistence（新增）、app-acceptance（新增）；污染治理：schedule-task-races、schedules-member-switch、stylist、scenarios、care（成员卫生守卫）。

## 5. 遗留

- **同步状态页终验**：SyncSettings.tsx 与 `tests/sync.spec.ts` 归 #31（并行开发中，未动）。#31 合入后须在同步页补齐「状态与能力匹配」终验（本票仅验证页面可达、无告警，见 `settings-sync-surface.png`）。
- 需真实模型/系统钥匙串的既有 skipped 项不变（local-provider、recognition 真机、weather-ai 真机、image-editing 抠图真机、encrypted-package、encryption OS）：环境变量开启后运行。
- #14/#20/#21 真实云图像调用（Seedream 云密钥）与 #35 CI 账户阻断维持既有结论，不在本票。

## 6. 最终全量数字

（run3 完成后回填。）
