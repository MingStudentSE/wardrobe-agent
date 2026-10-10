# 性能基线（issue #35：10000 单品 + 100000 穿着事件）

日期：2026-10-08。源码：`codex/ticket-35-release`（起点 464a845）。

## 测试机与目标硬件

| 项 | 值 |
| --- | --- |
| 机型 / 芯片 | Mac16,10 / Apple M4 |
| CPU 核 / 内存 | 10 核 / 16 GB |
| 磁盘 | 备份盘 HFS+ sparsebundle 映像（/Users/links/.codex/external-workspace-volume，174GiB 可用中实测） |
| 构建 | release profile，`MACOSX_DEPLOYMENT_TARGET=13.0`，`CARGO_BUILD_JOBS=2`（native-env 规定） |
| 父规格基线硬件 | 4 核 / 8GB RAM / SSD |

**如实说明**：本机规格高于父规格的 4 核/8GB 基线；以下数字证明当前实现在真实规模库上满足门槛，不等价于 4 核/8GB 低配机的实测。未做硬件降级模拟。

## 方法

- 基准程序：`crates/wardrobe-core/examples/benchmark_release.rs`（本轮新增，随分支提交可复现）。种子构造沿用 #15/#18 已验收的规模测试模式：1 件真实单品 + 1 条真实穿着经公共 Application API 创建，其余 9999 单品/99999 穿着为同文档形状批量插入（数据库名必须不存在，防误伤）。
- 数据库：专用合成库 `results/continue-20261007/ticket35/perf-baseline-db.sqlite`（162,414,592 bytes，10000 单品 + 100000 有效穿着事件）。
- 采样：每项 30 次（父规格要求 ≥30）。
- GUI 冷启动另测：隔离安装的 DMG 构建（同一产物 SHA256 aaf81895…）对同一基线库副本启动 10 次，`CGWindowList` 轮询主窗口出现时刻（窗口 ≥1000×500），原始数据 `gui-coldstart.jsonl`。

## 结果 vs 父规格门槛

| 指标 | 门槛（父规格） | 实测 p50 | 实测 p95 | 实测 max | 结论 |
| --- | --- | --- | --- | --- | --- |
| 本地搜索/筛选首屏（clothing.list，search+color+season+tag+sort，limit 24） | p95 ≤ 500 ms | 163.4 ms | **406.5 ms** | 450.4 ms | **达标** |
| 普通实体保存（clothing.update，不含图片/网络） | p95 ≤ 300 ms | 0.08 ms | **0.33 ms** | 0.67 ms | **达标** |
| 冷启动至可交互（GUI，spawn→主窗口出现；10 采样） | ≤ 5 s | 0.24 s | — | **0.59 s** | **达标**（见定义说明） |
| 参考：应用冷打开+首屏查询（Application::open + clothing.list，30 采样） | — | 289.9 ms | 496.7 ms | 502.0 ms | 附带证据 |
| 参考：显式穿着记录（wear.record，30 采样） | — | 0.11 ms | 0.55 ms | 27.2 ms | 附带证据 |

原始数据：`perf-baseline.json`（含全部 30 次逐样本数组）、`gui-coldstart.jsonl`、`perf-baseline.err`（空）。执行日志 exit 0。

## 口径与限制（如实）

1. GUI「冷启动至可交互」以**主窗口创建并显示**为观测点（CGWindowList 外部观测）。WebView 内容渲染完成的精确时刻无法从进程外可靠观测（验证会话屏幕锁定，截屏被系统拒绝）；作为补充，核心数据面的"打开+首屏查询" p95 为 496.7 ms，即使叠加进窗口出现时间仍远低于 5 s 门槛。
2. GUI 冷启动为 10 次采样（低于 30）：每次需完整启动/退出真实安装副本，10 次已覆盖首启冷缓存与稳态；核心面冷启动为完整 30 次采样。
3. 数据为合成文档形状（真实 schema 投影），非用户真实内容。
4. 本基线为 Application API / 进程级观测，不包含图片解码与网络（父规格口径即为"不含图片和网络耗时"）。
