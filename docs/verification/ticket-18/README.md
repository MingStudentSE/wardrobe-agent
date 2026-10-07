# #18 · 偏好学习作者最终验证

候选待独立验收；没有 push 或关闭 Issue。工作树 `/tmp/wardrobe-ticket-18`，分支 `codex/ticket-18`。产品源码冻结于 `4095912`，完整测试及截图冻结于 `0d25a89`，包含 accepted integration `b6ee5c8`。最终证据整理不修改产品源码。

## 最终同源结果

- `final-async-workspace.log`：245 passed / 13 ignored / 0 failed；fmt、clippy `-D warnings`、同源 CLI 编译退出 0，作者 owner handle89440终态0。
- `final-async-entry.json`：真实限定客户端 CLI/stdin MCP 持久入队、重开查询、显式 worker、terminal profile 与共享推荐评分；临时授权令牌不进入证据。
- `final-async-os.log`：真实临时系统 Keychain 加密6项、双安装文件包3项通过，handle2137终态0。单列OS测试，不计入默认Rust通过数。
- `full-gui-confirmed.log`：58 passed / 3 skipped，handle28945 owner终态0，退出旁证0。真实已有许可Qwen模型、公开合成库存、OS GUI flags已启用。跳过媒体/视觉外部门，不计为通过。
- `final-async-ui-build.log`：前端构建通过。截图来自同源浏览器GUI，320/1280实际来源与操作已检查；没有宣称原生Tauri窗口或其他OS验证。
- 最终非空10k/100k学习与备份往返 `final-async-scale.log`：handle7123 owner终态0，1 passed /0 failed、79.14s。立即enqueue 0ms；65显式/3推断，每推断100k来源UUID，profile11,726,241bytes；重算4758ms、共享排名4824ms；create2276/preview20902/restore39506ms。源文件删除后跨安装restore/reopen，10k衣物/100k实际wear、显式权重与stale推断保留。原规模日志保留用于对照。单次样本不代表#35 p95。

## Issue范围与公共合同

显式偏好与推断信号分开显示、保存来源及算法版本。显式六维权重可编辑、删除、查看分页历史；推断来自有效反馈和实际穿着的当时不可变快照。衣物当前字段变更不改旧来源，correct/revoke按有效修订重算。算法 `explicit-and-events-v1`：like +2/dislike −2，skip与其他反应0；实际有效wear +1；overall N−3作用全部特征、comfort作用材质、style作用颜色/图案/风格，各事件特征去重，−5..5截断。显式同特征覆盖推断，未知尺寸等事实不会补写。

删除持久 suppression，重置采用UTC事件记录时间 cutoff；普通重算不会悄悄恢复被忘记的旧信号。明确选择完整历史重建才清除边界。学习评分已实际改变共享规则排名，同时固定、排除、成员权限、最新care/day-exclude与发布有效性硬约束保留。学习管理、接受推荐及保存模板不会隐式写Wear。

`learning.recompute`立即返回持久job_id，重开Application可查询；CLI/MCP明确执行 `learning.worker.run`。GUI立即显示任务、轮询、继续、取消及中文状态。DEFERRED快照计算和大型序列化位于写事务外，短IMMEDIATE发布校验lease/cancel、reset/suppression和O(1)来源版本；迟到reset/cancel/fact修订不覆盖新意图。worker与相关job操作在成员授权后检查原始recompute权限，防止worker-only授权解除重置边界。确认成功的新学习意图刷新推荐缓存；未知503保留原operation_key去重。

DB20实际五业务表与缺表修复：learning_memories/profiles/state/history/receipts。local_source_versions和installation generation只属本机派生守卫，不portable。备份明确五表白名单、版本化DB19旧23表兼容、DB20缺表拒绝；typed validator校验权重/维度/修订/来源/跨成员receipt及reset状态。真实重签恶意包恢复拒绝且保留当前数据。profile仅顶层local字段剥离，嵌套未知业务证据保留。外来restore推断标stale，显式权重保留，须本机重算后参与评分。

#26文件包明确增加安全推荐intent/proposal、feedback快照/receipt/展示图引用及五学习业务事实。真实双配对安装解密读取验证来源与receipt引用完整、局部守卫和原图/本地路径/私有jobs/providers/钥匙缺席；读取不导入事实，不称完整同步。#27 transport由其owner后续集成已验收projection/validator。

## 失败与修正证据

`final-async-gui.log`原完整56 passed /3 skipped /2 failed保留：大额价格测试全局status locator遇到另一区域库存加载status，导致已保存测试衣物未清理，随后分页真实26与基准25冲突。只限定保存ack到#entry，并等待既有trash清理完成；不改分页oracle或产品。`final-gui-price-pagination.log`7项序列green，之后完整58项green。先前learning GUI disabled/remount错误与运行期间编辑造成HMR的可能性明确保留，未当作已证实产品bug。

实际TCP/SQLite公共异步、授权、并发和portable红绿日志保留。原同步checkpoint数据仅历史证据，不代替以上异步结果。资源清理仅已终态独占缓存项目test executable，路径/bytes/lsof记录 `final-async-cache-cleanup.json`；保留CLI/libs/models/其他缓存。

没有用户数据、真实用户钥匙、定位读取、付费模型或订阅购买。许可Qwen实际输出仅证明公开样本合法组搭协议，不代表预测准确率。完整Beta、五平台#35及CI账单阻断仍保留，待独立merger审查后再由root决定集成/关闭。

## Completed vertical evidence

- explicit-red.log: learning.set unsupported; explicit-green.log durable source/version/null inferred/idempotency/reopen and0wear. explicit-compile-error.log preserves initial Rust compile errors, not behavior red.
- recompute-red.log → recompute-green.log: known like+overall5+actualwear source snapshots yieldsblue5; repeatedsource weights identical,wearfacts unchanged.
- ranking-red.log → ranking-green.log: learned/explicitblueweight changes sharedranking; fixedred/excludedblue hard constraints prevail.
- management-red.log → management-green.log: appendeditCAS, delete suppression survivesordinaryrecompute, reset cutoff preventsoldfactsreappearing; include_prior=true explicitly rebuilds them, preservingactualwear andexplicitprefs.
- history-red.log → history-green.log: garmentcurrentblue→red edit doesnot rewritebluewearsnapshot; effectivewearrevocation removes learned signal; paginatedhistory reads distinctversions.
- score-red.log → score-green.log: skipalone contributesnothing, comfort5cotton+2 vsstyle1blue−2, optionaloverall actsallfeatures. Algorithm definedADR0025, not predictiveaccuracy.
- duplicate-red.log → duplicate-green.log: conflicting explicitroots foronefeature must useedit; twoindependentconnections sameexpectedrevision exactlyonewins.
- public-contracts.log: member isolation, limitedoperation/member authorization, invaliddimensions/weights andDB20 actualmissingtablesrepair.
- gui-red.log: visible memoryentrymissing; gui-red-cleanup-timeout.log preservesfirstmaskedcleanup timeout. gui-closed-details-fixture-error.log tried hiddencheckbox withoutopeningdetails; fixedtestclicksactualsummary. gui-readable.log:2 actualGUIcases passed: explicitsave/edit/reload/delete/reset, realexistingwearsource/view/correction→ranking, forget/recompute/fullrebuild, factspreserved, sevenwidths. Source/event screenshots320/1280 reviewed.
- actual-entry.json + reproduce-entry.py: actuallimitedCLI/stdinMCP all7learningops/recommendation/wearread,8paginatedmutations,oneexistingwearpreserved. Initialfixtureexpected9afterperforming8, entry-fixture-count-error.log retains correction; notdomainfailure.


## Independent acceptance evidence (2026-10-07, root-packaged)

Author claims above are preserved verbatim and were not modified. Independently accepted evidence is packaged in `independent-20261007/` (see its README and MANIFEST.sha256): source business commit `7a4e3a2`; reset-timestamp red→green (2 tests) with workspace 247 passed / 13 ignored, fmt/clippy exit 0, 10k/100k scale pass 73.79s (single run, not p95); rebuilt CLI/MCP reproduction exit 0 with 4 receipts queued→succeeded; pre-reset-change OS Keychain 6 + encrypted-package 3 passes (label source `cd7ef72`); final persistent-owner GUI run 58 passed / 3 skipped on real licensed local Qwen endpoint 18087; four final learning screenshots; two separate review-axis reports (standards/spec, base `b6ee5c8...7a4e3a2`, documented standard violations 0, spec missing/wrong 0) and a preserved, traceable incomplete OpenCode GUI run (runner disappeared at test 12 — neither a pass nor a product failure). Whole-Beta, native-3-OS, CI billing, p95 and model-quality claims remain open; no push/merge/close.
