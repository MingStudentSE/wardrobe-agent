# Ticket #34 验收截图索引

全部截图为真实 GUI 运行产物（Playwright `page.screenshot`，channel=chrome，真实 CLI 桥）。
本目录截图由 `tests/app-acceptance.spec.ts` 与 `tests/settings-persistence.spec.ts` 在通过路径上自动生成；
各业务 spec 的七档宽度截图落在 `docs/verification/ticket-XX/`（见下表"关联目录"列）。

## 本目录（docs/verification/ticket-34/）

| 截图 | 验收项 | 产生来源 |
| --- | --- | --- |
| keyboard-focus.png | 键盘焦点（skip link、侧栏导航焦点态） | app-acceptance「skip link、侧边导航与设置目录支持键盘聚焦…」 |
| font-scale-200-375.png / font-scale-200-1280.png | 字体缩放 200%（root font-size 32px）下 375/1280 不破版 | app-acceptance「root font-size 200% 时…」 |
| reduced-motion.png | prefers-reduced-motion 下无过渡动画且应用可用 | app-acceptance「prefers-reduced-motion 下…」 |
| empty-states-sync.png | 空状态逐页可见（含同步页表面，状态页能力归 #31） | app-acceptance「空状态逐页可见且中文提示明确」 |
| error-state.png | 桥接 503 → role=alert + 重新读取 | app-acceptance「加载中与错误状态可见…」 |
| offline-state.png | 离线（桥接不可达）→ 明确错误并可恢复 | app-acceptance「离线（桥接不可达）时…」 |
| long-name-card.png / long-name-wear-dialog.png | 中文长名称在卡片/详情/穿着弹窗完整可读 | app-acceptance「中文长名称在卡片、详情与穿着弹窗中完整可读」 |
| settings-model-persist.png | 集中设置：模型与连接持久化 | settings-persistence「模型与连接：新增配置经重启后读回一致」 |
| settings-preferences-persist.png | 集中设置：穿衣偏好持久化（用户声明来源） | settings-persistence「穿衣偏好：…跨重启读回」 |
| settings-weather-persist.png | 集中设置：自动天气配置持久化 | settings-persistence「天气：自动天气配置保存并跨重启读回」 |
| settings-schedule-persist.png | 集中设置：定时计划与通知持久化（暂停态） | settings-persistence「定时计划与通知：…」 |
| settings-backup-persist.png | 集中设置：备份目录与数据目录读回 | settings-persistence「备份与数据目录：…」 |
| settings-encryption.png | 集中设置：加密与设备页可达且无错误告警 | settings-persistence「加密与设备、同步、CLI/MCP、任务中心…」 |
| settings-sync-surface.png | 同步设置页当前表面（状态页终验归 #31） | 同上 |
| settings-tasks.png | 任务中心反映真实后台任务 | settings-persistence「任务中心反映真实后台任务并刷新」 |

## 关联目录（各业务 spec 的七档宽度与流程截图）

| 目录 | 覆盖 | 主要来源 spec |
| --- | --- | --- |
| docs/verification/ticket-07/ | 单品图片编辑/归并（320/1280） | image-editing.spec.ts |
| docs/verification/ticket-09/ | 穿着记录（320/1280） | wears.spec.ts |
| docs/verification/ticket-13/ | 识别建议审核表单（320/1280） | recognition.spec.ts |
| docs/verification/ticket-15/ | 推荐面板与提案（320/1280） | recommendations.spec.ts |
| docs/verification/ticket-16/ | 天气配置与 AI 推荐（320/1280） | weather-ai.spec.ts |
| docs/verification/ticket-17/ | 推荐反馈（320/1280） | feedback.spec.ts |
| docs/verification/ticket-04-ui-*.png（/tmp，全档） | 管理控件七档可达 | clothing-management.spec.ts |

## 七档宽度溢出/遮挡审查

`tests/responsive-audit.spec.ts`（工作区×4、穿搭×6 页签、记录×3 页签、设置×8 卡片及全部弹窗）+
`tests/redesign.spec.ts`「supported widths…」+ `tests/members.spec.ts`「member controls…」等
在 320/375/414/768/1024/1280/1440 七档断言 `scrollWidth<=innerWidth` 且关键控件可见，
无独立截图，以自动化断言与全量结果为证（见 acceptance-report.md §6）。
