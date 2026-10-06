# #9 独立验收范围审计

2026-10-07；审计人未实现#9。本轮只读，不修改仓库、不重跑CI、不关闭issue。

## 判定

**可以按功能任务关闭#9，并开放依赖它的#10/#11；没有发现本票四项功能验收仍缺少实际证据的阻断条件。** 必须在关闭说明中明确“F12及F27穿着部分功能验收通过；新代码五平台构建和各平台原生GUI/安装/升级/恢复尚未验收，由#35继续阻断Beta发布”。这不是宣称全Beta/F24通过，不撤销父规格三平台要求。

依据：#9本身的验收项要求穿着动作、次数/天数、幂等、不可变历史和已知跨时区fixture，没有要求必须等待五平台CI或安装包后才能结束功能任务。父规格第5/7节将三平台同时进入**Beta发布门槛**；F24与#35明确承担实际安装/升级/恢复、原生GUI/CLI/MCP、性能与全F01–F30验收。此前根任务附加的新五平台CI关闭门槛是工作流加严，而不是#9验收项中的独立功能行为；将其保留为#35发布阻断，不删任何用户功能、平台或真实运行承诺。

## 当前证据版本

当前root `eb1c04fcc33b686f716d55b1f57bf356dcec61fa`。本代理刚在独立合并工作树真实构建并运行的源为 `beefa47a8acf42334286ac75ed4715b9baa6c848`（root4fa138f + #21的88df1ad + root240c2ea）；eb1c04f只追加独立验收报告与日志，没有修改业务源码。日志在root `docs/verification/ticket-21/independent/{workspace,gui}.log`，报告记录同一构建来源。

- 最新联合源：workspace **110 passed / 0 failed / 8 ignored**。日志明确逐项执行并通过`tests/wears.rs`全部**8项**；wear没有ignored项。独立CLI/MCP穿着测试也通过。
- 最新CLI源码`cargo build -p wardrobe-cli`成功后，真实Chrome→Vite桥→该CLI→临时SQLite GUI **33 passed / 2 skipped**，3项wear GUI全部通过；跳过是实际抠图模型与SmolVLM环境，不涉及穿着。没有借旧CLI、业务mock或未运行断言作为通过。
- 原#9报告85Rust/27GUI与独立merger的8穿着/资产/图片/导入/搭配回归是补充历史证据，非当前唯一依据。
- **9070fa7旧五平台CI不能证明#9新源**；本审计不借它填新平台缺口。4fa138f新CI在runner任何步骤启动前因账户billing/spending限制阻断，既不是新代码跨平台成功，也不是源码测试失败。

## 逐项功能验收

| #9验收项 | 当前源的真实公共行为证据 | 判定 |
| --- | --- | --- |
| 单品/整套/模板今天穿/实际改穿、补记、更正撤销；未来日期拒绝 | `explicit_actual_wear_counts_events_and_local_days_without_template_side_effects`用单品与source_outfit版本记录；`corrections_and_revocation_append_history_recompute_and_preserve_exact_retry_receipts`换实际衣物、补日期、追加更正撤销、重启；`public_history_keeps_template_snapshot_and_refuses_cross_member_and_future_facts`拒绝未来日期。3项wear GUI分别实际补记→换装→改穿→撤销/追加历史/刷新、模板今天穿与单品穿过显式确认、响应丢失重试及未来日期错误。 | 已通过 |
| 同日同衣两事件为两次一天；幂等重试不重复；保存推荐/搭配不记穿着 | 首条core测试同日单品+整套给出2次/1天，重复operation_key收据相同；并发更正与不同意图复用key测试通过。GUI真实提交后丢HTTP响应再重试只计1次；模板保存后次数为零。outfit公共行为及CLI/MCP保存/版本测试、flatlay/tryon本地保存测试均未产生穿着。推荐尚未实现，未把未来推荐入口视为通过；现有保存操作不生成WearEvent，推荐后续必须沿此领域边界验收。 | 本票已有行为通过；推荐未来行为仍属其自身任务，不能伪称已验收 |
| 模板/单品修改删除不改历史快照；跨时区与更正重算已知fixture | core历史测试在记录后改单品名、删单品、删模板，再只更正日期仍返回旧单品/模板名；原事件仍可get，历史追加。模板版本测试在改单品/模板后v1完全等于保存前对象，重启仍一致。WearEvent存独立JSON、只INSERT追加事件，读取不重新join当前模板/单品；模板编辑不改已存event。两次跨UTC午夜fixture：上海2020-01-01 00:30→2019-12-31T16:30Z；檀香山2019-12-31 23:30→2020-01-01T09:30Z，2次/2当地日。纽约DST缺失/歧义时刻拒绝；更正/撤销重算到换穿衣物1次/1天和空计数。 | 已通过；模板编辑→旧WearEvent直接assert并非独立单一测试，而由已运行版本/历史测试与独立持久事件路径联合证明，不虚构额外测试 |
| 外部行为、实际结果、术语/决策，无占位 | Application+SQLite、独立CLI及授权stdin MCP、真实GUI桥均有当前实际通过日志；GLOSSARY的WearEvent、ADR0011追加更正/CAS/时间语义、ADR0010模板快照及report/merger-report齐备；资产历史全部图片和detach后引用保留实际测试通过。 | 已通过 |

F27的创建/克隆/编辑Lookbook由#8完成：当前联合源5项outfits公共行为测试、2项outfits GUI与独立CLI/MCP保存历史版本测试均实际通过；#9补上模板今天穿及实际改穿。不存在靠关闭#9把未实现F27能力删掉的情况。

## 时区与平台证据边界

Rust公共入口使用显式`local_date`和IANA`timezone`，`chrono_tz::Tz`解析并用其规则算UTC/当地今日，fixture不依赖当前OS设为上海、檀香山或纽约。测试证明同一共享Rust实现和SQLite记录的时区语义、DST拒绝、当地日计数；这是#9明文要求的“已知fixture”，不是只模拟业务函数结果。

GUI默认使用JS `Date`取得本机日期、`Intl.DateTimeFormat().resolvedOptions().timeZone`取得默认时区，同时有可编辑日期/时区输入。现有macOS Chrome测试证明显式输入和本机今天穿流程；没有证明Windows/macOS Tauri/Linux原生GUI在系统时区切换、午夜、DST转换下默认值均正确。该跨平台原生环境验证仍待#35；也不能因chrono-tz可移植就声称五平台已实际运行。

## 保持未完成的发布门槛

#35保持OPEN：最新集成的Windows11 x64/macOS13+ arm64+x64/Ubuntu22.04+24.04构建与原生GUI/CLI/MCP，EXE/DMG/DEB干净安装、升级/卸载数据保留与恢复，10000单品/100000事件/至少30次性能采样，完整F01–F30、许可/签名/公证/依赖与产物报告。相关目标必须最终真实通过，账户限制恢复后运行最新源码，而非用旧9070结果代替。

关闭#9是对已经完成的功能节点记账；若根任务继续坚持“每票先过新五平台CI”的额外规则，则可以行政保持OPEN，但应准确标记剩余仅是该附加规则的验证，不能说穿着功能缺失或Beta平台门槛已取消。本审计建议释放#10/#11功能研发，保留所有发布阻断。
