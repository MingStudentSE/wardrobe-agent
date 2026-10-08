# 最终门禁清单（final-gate）——F01–F30 对照现状

日期：2026-10-08。**本文件只是发行终验的准备稿，不勾选任何发布门槛**；最终勾选须在 #34 合入后由 #35 终验完成。
issue 状态为 2026-10-08 经 GitHub API 核对（#14/#20/#21/#34/#35/#39 OPEN，其余功能票 CLOSED）。

图例：
- 【已验收】= ticket 已关闭且验收评论含该 F 的真实证据；
- 【本轮证据】= 本分支新增可复现证据（docs/verification/ticket-35/ 与 results/continue-20261007/ticket35/）；
- 【待 #34 终验】= 等 #34 合入后复核；
- 【待用户云密钥】= 真实云图像调用验收未完成；
- 【#35 终验】= 属本票验收动作，终验时执行。

| F | 最低验收（摘） | 主责票（状态） | 验收证据链接 | 现状与缺口 |
| --- | --- | --- | --- | --- |
| F01 | 两成员隔离，三入口一致 | #3 CLOSED | [#3 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/3#issuecomment-6021819408) | 【已验收】 |
| F02 | 无 key 录入、批量重试、重启续跑 | #4 CLOSED | [#4 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/4#issuecomment-6021821640) | 【已验收】 |
| F03 | 字段/分类/购买信息往返 | #5 CLOSED | [#5 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/5#issuecomment-6022603097) | 【已验收】 |
| F04 | 搜索/筛选/收藏/归档/回收站 | #6 CLOSED | [#6 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/6#issuecomment-6022988498) | 【已验收】 |
| F05 | 真实视觉建议+人工纠错 | #13 CLOSED | [#13 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/13#issuecomment-6023980829) | 【已验收】（真实云视觉部分与 #14/#20/#21 同源限制，本地视觉模型路径有真实证据） |
| F06 | HEIC/方向/白衣样图/压缩保原图 | #7 CLOSED | [#7 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/7#issuecomment-6023675746) | 【已验收】+【本轮证据】发行包 CLI 真实 HEIC 解码（standalone-runtime.json：64×32 HEIC 导入/重启读回） |
| F07 | 商品图真实任务/预览采用/重生成 | #14 **OPEN** | [real-cloud 报告](../ticket-14/report-real-cloud.md) | 【已真实调用验收】2026-10-08 真实 Seedream 5.0 Flash 两次生成（显式采用+重生成不采用保留旧图、来源追溯成链），证据 results/continue-20261007/real-ai-acceptance/ |
| F08 | 手工搭配/编辑收藏/离线拼贴 | #8 CLOSED | [#8 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/8#issuecomment-6022990240) | 【已验收】 |
| F09 | 在线/离线推荐输出真实可用 ID | #15 CLOSED | [#15 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/15#issuecomment-6024834540) | 【已验收】 |
| F10 | 固定/替换/换套/硬约束 | #15/#16 CLOSED | [#16 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/16#issuecomment-6025788843) | 【已验收】 |
| F11 | 反馈分类保存，不增穿着 | #17 CLOSED | [#17 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/17#issuecomment-6026218600) | 【已验收】 |
| F12 | 穿着补记/换装/更正/撤销/时区 | #9 CLOSED | [#9 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/9#issuecomment-6023922046) | 【已验收】 |
| F13 | 洞察与人工计算一致 | #11 CLOSED | [#11 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/11#issuecomment-6024374688) | 【已验收】 |
| F14 | 定时/暂停/通知/休眠补执行 | #25 CLOSED | [#25 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/25#issuecomment-6042823743) | 【已验收，有限定】真实 macOS 系统通知实际显示、后台/登录自启动的安装态验证留有限定记录（ticket-25 README "原生…门仍 pending" + 9ea567e 记录）；#35 终验时按现状复核 |
| F15 | AI flat-lay 真实成功 | #20 **OPEN** | [real-cloud 报告](../ticket-20/report-real-cloud.md) | 【已真实调用验收】2026-10-08 真实三展示图 flat-lay 生成+保存（blob 哈希与存证一致、wear.list=0、仅本机）；离线拼贴兜底已有证据 |
| F16 | 真实试穿/对比/仅本地 | #21 **OPEN** | [real-cloud 报告](../ticket-21/report-real-cloud.md) | 【已真实调用验收】2026-10-08 真实 AI 合成模特+三衣物试穿、compare 前后哈希闭环、仅本机且无穿着记录 |
| F17 | 五类场景工具链 | #22/#23/#24 CLOSED | [#22](https://github.com/MingStudentSE/wardrobe-agent/issues/22#issuecomment-6042322433) [#23](https://github.com/MingStudentSE/wardrobe-agent/issues/23#issuecomment-6044395758) [#24](https://github.com/MingStudentSE/wardrobe-agent/issues/24#issuecomment-6044175669) | 【已验收】（各票范围内的云模型真实调用限制同上） |
| F18 | 记忆/画像重置 | #18 CLOSED | [#18 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/18#issuecomment-6029230182) | 【已验收】 |
| F19 | GUI 关闭后 CLI 独立读写 | #33 CLOSED | [#33 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/33#issuecomment-6038676238) | 【已验收】+【本轮证据】发行包 CLI 干净环境 --version/--help/授权/HEIC/MCP（standalone-runtime.json） |
| F20 | MCP 独立启动/越权拒绝 | #33 CLOSED | 同上 | 【已验收】+【本轮证据】unauthenticated CLI/MCP 双拒绝、MCP initialize/tools/call 通过 |
| F21 | 三后端同步契约/冲突收敛 | #27/#28/#29/#30/#31 CLOSED | [#27](https://github.com/MingStudentSE/wardrobe-agent/issues/27#issuecomment-6040817452) [#28](https://github.com/MingStudentSE/wardrobe-agent/issues/28#issuecomment-6044635452) [#29](https://github.com/MingStudentSE/wardrobe-agent/issues/29#issuecomment-6043413369) [#30](https://github.com/MingStudentSE/wardrobe-agent/issues/30#issuecomment-6042614950) [#31](https://github.com/MingStudentSE/wardrobe-agent/issues/31#issuecomment-6046620683) | 【已验收】（真实 S3/WebDAV 后端往返在各票记录） |
| F22 | 备份导出/恢复预览/格式版本 | #32 CLOSED | [#32 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/32#issuecomment-6027784159) | 【已验收】 |
| F23 | 集中设置/密钥不明文 | #34 **OPEN** + #31 | — | 【待 #34 终验】（另一分支并行收尾中）；密钥生命周期部分见 #31 |
| F24 | macOS 发行/升级/卸载/跨平台审查 | #35 OPEN（本票） | — | 【#35 终验】本轮已完成：DMG 构建、干净安装/升级(schema20→23)/卸载数据保留、签名记录、架构审查、性能、隐私、许可清单；终验待 #34 合入后执行。【本轮证据】多平台本地构建验证基础设施：Ubuntu 24.04 ARM64 VM 端到端（构建→deb 修复→dpkg 安装→真实 HEIC 烟测）、x86_64 TCG 可行性实测、Windows ARM64 VM 管线（详见 docs/verification/ticket-35/multiplatform-infra.md；Windows 端到端待 ISO 回填后按 §7.0 收口） |
| F25 | 洗护阈值/清洗/提醒/排除 | #10 CLOSED | [#10 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/10#issuecomment-6024210500) | 【已验收】 |
| F26 | 多图/旋转/抠图/恢复/主图 | #7 CLOSED | [#7 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/7#issuecomment-6023675746) | 【已验收】+【本轮证据】发行包 CLI 真实 U2NetP 抠图 1200×1600、SHA256 校验、缓存重启、MCP 同 hash（standalone-cutout-runtime.json） |
| F27 | Lookbook/Studio 模板与改穿 | #9/#38 CLOSED | [#9 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/9#issuecomment-6023922046) | 【已验收】 |
| F28 | 单品配对生成/保存/拒绝失效 ID | #19 CLOSED | [#19 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/19#issuecomment-6036428481) | 【已验收】 |
| F29 | 关闭内置 AI 后外部写回 | #33 CLOSED | [#33 评论](https://github.com/MingStudentSE/wardrobe-agent/issues/33#issuecomment-6038676238) | 【已验收】+【本轮证据】standalone CLI limited grant 越权拒绝 |
| F30 | 收藏/购买/评分/排除/Try Another | #17/#13/#24 CLOSED | [#17](https://github.com/MingStudentSE/wardrobe-agent/issues/17#issuecomment-6026218600) [#13](https://github.com/MingStudentSE/wardrobe-agent/issues/13#issuecomment-6023980829) [#24](https://github.com/MingStudentSE/wardrobe-agent/issues/24#issuecomment-6044175669) | 【已验收】 |

## 本轮发行工程证据索引（results/continue-20261007/ticket35/）

| 证据 | 文件 |
| --- | --- |
| 构建全日志（cargo release→媒体分发→npm→tauri build） | build-release.log（STEP1_EXIT=0，DMG 段 bundle-dmg-final.log，均 exit 0） |
| 产物与 SHA256 | artifacts/（DMG aaf81895…、CLI f2d7258c…、SHA256SUMS.txt） |
| CLI 独立运行（HEIC/授权/拒绝/MCP） | standalone-runtime.json（exit 0） |
| CLI 真实抠图（模型下载校验/缓存重启/同 hash） | standalone-cutout-runtime.json（exit 0） |
| 安装/升级/卸载 | install-test/（logs/、screenshots/、window 记录）+ docs/verification/ticket-35/install-upgrade-uninstall.md |
| 性能基线 | perf-baseline.json（30 采样）、gui-coldstart.jsonl（10 采样）+ performance-baseline.md |
| 隐私抽查 | privacy-scan.json + privacy-spot-check.md |
| 许可清单 | THIRD-PARTY-LICENSES.md + license-appendix.md + cargo/npm 原始行数据 |
| 真实云图像调用（#14/#20/#21，2026-10-08） | results/continue-20261007/real-ai-acceptance/（logs/ 全命令日志、evidence/ 生成图 SHA256）+ docs/verification/ticket-14|20|21/report-real-cloud.md |

## 终验前必须完成的缺口汇总

1. 【待 #34 终验】F23 与全应用 UI/设置回归——#34 合入本集成线后对本候选复核。
2. F07（#14）、F15（#20）、F16（#21）真实云图像调用成功证据——已于 2026-10-08 闭合：真实 Ark Seedream（doubao-seedream-5-0-flash-260915）调用验收完成，见 docs/verification/ticket-14|20|21/report-real-cloud.md 与 results/continue-20261007/real-ai-acceptance/；三票 issue 关闭动作仍归各票。
3. 【#35 终验】F24 收口与 F01–F30 对照表最终勾选；F14 的安装态通知/自启动按实际现状复核并如实记录。
4. 产品许可证决定（见 THIRD-PARTY-LICENSES.md 第 1 节）。
5. （分发环节）签名公证、CI 云端复现构建（账户账单恢复后）。
