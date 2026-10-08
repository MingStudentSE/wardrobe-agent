# ADR 0033：全应用 UI 验收的跨工作区模态与可达性裁决

状态：已实施；分支 `codex/ticket-34-ui`（基于集成分支 tip 846a6b1），证据在 `docs/verification/ticket-34/` 与 `wardrobe-execution/results/continue-20261007/ticket34/`。

## 背景

四工作区重设计（#38）合并后，约 55 个功能 spec 仍使用旧 UI 选择器。#34 统一把导航/定位更新到新 UI（导航按钮 + 工作区页签 + SettingsHub 卡片），保留全部行为断言。修绿过程中发现两个跨页面可用性缺陷，均属"不得依赖桌面宽度或悬停完成关键流程"（父规格 #1 / docs/04）的违反，按既有 Atelier 视觉体系就地修复组件，不引入新主题。

## 决策

- **模态弹窗不得在宿主面板隐藏后继续全页遮挡。**穿搭库「调整穿搭」弹窗内「模板今天穿」会切换到记录工作区；宿主面板隐藏（`display:none`）后，`showModal()` 的弹窗仍以全页 backdrop 拦截一切指针与键盘输入，应用整体假死。裁决：跨工作区的穿着意图必须先显式关闭来源模态（Lookbook 在触发 `onWear` 前 `setEditorOpen(false)`），主界面随后自行的关闭逻辑保持不变。同类流（单品详情「今天穿了」）本就先关弹窗，不受影响。
- **侧栏必须内部可滚动，窄屏不得隐藏成员切换。**重设计的侧栏为 `100dvh` sticky，侧栏底部「当前衣橱 → 管理成员档案」展开的表单渲染在视口之下且不可达（键盘与触摸均无法完成成员管理）。裁决：`.rail` 增加 `overflow-y:auto; overscroll-behavior:contain`，展开内容在侧栏内滚动可达。同时撤销 ≤768px 隐藏 `.rail-bottom`（成员切换器）的旧规则——成员切换是关键流程，移动宽度下保留紧凑形态、隐藏装饰性说明。
- **旧功能 spec 的到达路径统一为新 UI，行为断言逐条保留。**协议 fixture、成员隔离（afterEach 切回默认成员，新增 `tests/support/member-hygiene.ts` 供竞态类规格复用）、幂等重试、七档宽度、不产生穿着等断言一律不动；只改导航（`主要导航` 按钮 → 工作区页签 / 设置目录卡片）、模态弹窗流程（`＋ 记录穿着`、`＋ 记录洗护`、`＋ 新增衣物 → 批量录入`）与汇总口径的动态读取（共享 dev server 上库存随规格累积，计数以 `clothing.list`/`insights.summary` 真实读取为准）。
- **浏览器预览的图片通道如实声明。**开发桥上传端点仅收 JPEG/PNG/WebP；HEIC 走桌面原生路径选择（`assets.import` 本机路径）。涉及多图流程的规格改用两张 PNG 保留多图断言，不伪装浏览器可处理 HEIC。

## 测试与证据

- 全量 GUI（`apps/desktop`，workers=1，真实 CLI 桥）：修复前 61 failed / 34 passed / 8 skipped；修复后全绿（详见 `docs/verification/ticket-34/acceptance-report.md`）。
- 组件修复的回归锚点：`tests/wears.spec.ts`（模板今天穿后页面可继续操作）、`tests/members.spec.ts`（七档宽度下成员控件可见）、`tests/responsive-audit.spec.ts`（全工作区 + 全设置页七档溢出/遮挡审查）。
