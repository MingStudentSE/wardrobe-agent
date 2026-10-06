# #11 独立合并审查与联合验证

2026-10-07，macOS arm64。独立工作树 `/tmp/wardrobe-merge-11`，branch `codex/merge-11`，从root295e3fd合入实现者2c1cd18，再合rootcare1b9ad4a。Rust完整检查源码57668e9；随后GUI加载修复源码0717680，Rust业务路径未变，联合migration版本断言又单独复验通过。最终仅证据提交不改变业务实现。已合最新纯进度root6dabae2。

## 规格审查

Issue11/F13与ADR0018：当前active+archived库存分母，deleted另列；多颜色每件每色一次，未知颜色单列，色桶总和允许超过100%。完整手算六件/四事件验证总量5、红/未知各40%、蓝/绿各20%，A3次/2天，AB组合2次/1天；当前闲置90/91入、89不入，从未穿过单列。更正/撤销重新派生，保存模板不推断穿着。当前库存明确不随过去截止日还原。

公开Application在DEFERRED读取事务内计算所有投影；真实SQLite、跨时区截止、empty denominator、当前与历史删除范围、成员隔离、授权readonly不能configure、两连接CAS和跨重启持久偏好均复验。统计口径没有发现缺项；独立GUI随后发现的重载查询时序缺陷已修复，见下文。UUIDv7/创建时间/更新修订、偏好audit/outbox在同事务；不保存派生计数。

## 标准审查与合并修正

保留care与insights两套导航/组件、GLOSSARY术语、Application dispatch/descriptors与实际表迁移。DBmax14，care13、recognition12与encryption10真实表均初始化。保留care独立merger的严格更正意图校验及读取一致性修复。新增DB14同时drop care_config与insights_preferences后重开：care历史清洗仍在、洗后次数0；insights实际穿着频率1、默认90；两偏好可保存，recognition可查，全局版本不降低。

首轮完整检查发现care资产留存测试硬编码版本13，在本轮升级14后失败；保留schema-red.log，将预期改为公开DATABASE_SCHEMA_VERSION。这是跨票测试假设修正，资产/manifest留存断言本身通过，没有把测试失败冒称产品数据损坏。另补Hallmark六轴持久critique注释，不改视觉或行为。完整GUI发现保存89天后重载可能显示初始90：打开洞察时父级仍loading，自动查询提前return且未恢复；初始member为空的旧组件也可先被打开，随后key重挂丢掉展开。新增真实延迟clothing.list的确定性红灯，修复为成员已知后才呈现洞察，自动展开查询等待disabled解除、每次展开只请求一次。三项洞察GUI已通过（原2项+新的延迟初始化，并断言只有一次真实summary请求）；首次完整GUI与确定性loading红灯均保留。复用现有Atelier tokens、可读文字+meter和触控入口；320图已实际查看，窄屏不溢出、数值完整。

## 实际终态

- `cargo test --workspace --locked`：139 passed / 0 failed / 8 ignored，exit0。含insights6（5原公共行为+1独立联合migration）与授权真实CLI/stdin MCP单项；完整CLI21项通过。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`、fmt check、git diff check、TypeScript/Vite build：实际exit0。
- 独立洞察GUI三项已通过，真实Chrome→开发bridge→本源码CLI/Application/SQLite：手算图表/89-90-91/偏好重载重置/更正撤销/成员隔离、真实延迟查询锁/空态/错误恢复/焦点、提前展开后自动恢复查询；七宽度320/375/414/768/1024/1280/1440，320/1280截图已实看。
- 最终完整GUI：44 tests，40 passed / 0 failed / 4 skipped，exit0；包含三项洞察及三项洗护实际行为检查。模型/OS条件跳过不计通过。所有Rust/CLI构建先终态，之后序贯GUI，避免替换其正在调用的CLI。

原生Tauri窗口本轮未操作；8项Rust ignored为既有真实OS/模型门槛，本轮未执行，不计native或平台成功。没有读用户key/照片，采用合成文字事实与固定日期。未push、关闭Issue或启动CI；五平台原生发行/性能/安装恢复等#35门槛仍未以此替代。

全部独立日志在 [merger-logs](merger-logs/)，实现者原始125Rust/37GUI证据另见report.md，不将旧总数当本轮联合结果。07缓存为本任务独占，debug0/jobs2/incremental0，native prefix复用；只清理已终态的旧项目测试exe，CLI/依赖/模型/其他活跃cache保留。
