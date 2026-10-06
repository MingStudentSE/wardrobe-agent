# #11 衣橱洞察验收

2026-10-07，macOS arm64；独立分支codex/ticket-11，基于root bf311ef，已合root识别1f39c5e及最终文档295e3fd。实际验收源码HEAD `5cf739403383c74e58e2bbd601914daae31afe17`，报告提交仅追加证据。未push/关闭issue。

共享Application / 真实临时SQLite / 独立授权CLI及stdin JSON-RPC MCP / Chrome→Vite桥→由当前源码构建的CLI，使用合成文字事实和固定历史日期，不读个人衣橱或用户密钥。

## 已验证行为

`insights.summary(member_id,as_of_date,timezone)`在同一读取快照内派生库存、类别/多颜色分布、实际频率、组合、久未穿和从未穿过。默认非删除库存（在用+归档），回收站数量另列。颜色每件每色一次，空色unknown/null，分母是库存数，合计可超过100%。频率包含已删除衣物的有效历史且有明确标记，次数/当地天数分别返回；没有实际记录的库存是0次/0天。组合按同一实际事件完整衣物集合，顺序和模板差异不重复分组。

`insights.configure(member_id,stale_days,expected_revision)`保存成员业务偏好，默认90，null重置；UUIDv7/schema/创建更新时间/修订，CAS防止覆盖，偏好/audit/outbox同事务。DB14新增真实偏好文档表，无派生计数表；初始化检查实际表，即使全局版本14仍修复缺表，不降低旧数据。未来同步白名单需含此偏好，当前未实现远端同步。

## 手算fixture

6件：A红蓝上衣、B红上衣、C未知色裤子、D绿外套（归档）、E未知色其他、F鞋（记录后删除）。有效事件：Jan1 AB、同日 BA、Jan2 AF、Jan3 D；截止2020-04-01上海。

- 当前库存5（在用4/归档1），回收站1；类别top2=40%，bottom/outerwear/other各1=20%。
- blue1/green1各20%，red2/unknown2各40%，色桶比例总计120%。
- A3次/2天，B2/1，C0/0，D1/1，E0/0，F1/1（不在库存）。4个有效根。
- AB集合2次/1天，AF1/1，D1/1。BA不另开组。
- A距离90天、B91、D89；默认阈值90只列A/B，从未穿C/E另列。阈值89列A/B/D，删除F不入当前久未穿列表。
- AF日期更正到Jan3，撤销Jan1第二次AB后：A2次/2天、B1/1、AF1/1，3个有效根；阈值90只列B。

额外fixture：上海Jan1 00:30在UTC为Dec31，查询Jan2 UTC间隔2自然日；wearDays仍按保存当地日。更早截止不计未来穿着，当前库存明确不随历史截止还原。空库分母0输出空桶。错误日期/时区/阈值拒绝，成员偏好隔离；readonly grant不能configure/不能跨成员查询。两Application连接CAS拒绝过期修订，重启保留同UUID/创建时间/修订。只知日期的穿着以原领域说明的中午锚点换算，界面不冒充真实分钟。

## 真正运行结果

- `cargo test --workspace`：**125 passed / 0 failed / 8 ignored**，本票5项core公共行为及1项授权CLI/MCP均执行通过。ignored是其他票的真实OS/模型环境检查，不当作本票或跨平台成功。
- 最新源`cargo build -p wardrobe-cli`成功后，`WARDROBE_ENCRYPTION_OS_TEST=1 WARDROBE_TEST_PORT=5215 npm run test:e2e`：**37 passed / 0 failed / 3 skipped**。本票2项均执行：完整手算数值/图表、89-90-91、配置重载/重置、成员切换、实际更正撤销重算、空态/真实查询加载与禁止成员切换、参数错误后修正、成功状态与焦点。3 skips为真实抠图、SmolVLM、实际识别模型门槛；真实Keychain GUI本轮执行。没有把HTTP协议fixture冒充真实模型效果。
- 320/375/414/768/1024/1280/1440七宽度无横向溢出，动作可见；320/1280截图实际查看。Atelier颜色、字号与触摸入口；meter有精确文字值，不仅依赖色彩。[控件八态视觉页](Insights.preview.html)仅视觉检查，不代替真实状态测试。
- `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、TypeScript/Vite build、`git diff --check`成功。

## red→green及修正记录

库存/频率首条公共测试先因unsupported_operation失败；第二条先因缺失stale_days/组合失败；CAS偏好测试先因expected_revision不支持失败，修复后4项通过，后补DB14实际缺表修复回归后5项通过。GUI首先因为没有洞察入口失败，之后真实数值通过；首轮末尾误用default别名作为select UUID值导致测试失败，修正测试。全套首轮又发现空态fixture未清理当前成员、污染后续媒体测试，停止该本机失败harness，补afterEach恢复默认成员后整套37项通过。上述是如实的测试隔离修正，不伪称产品曾存在相同缺陷。

构建使用独占07缓存、jobs2、CARGO_INCREMENTAL=0与DEV/TEST DEBUG=0，native prefix `/tmp/wardrobe-media-native/install`。磁盘不足前仅清理此独占缓存已经终态的旧项目测试可执行文件；未碰活跃root/09缓存、源代码、用户数据或release包。

## 未冒称完成的门槛

新源码五平台CI因账户billing/spending限制未启动，不重试或记成功；原生Windows/macOS/Linux发行、安装升级恢复、10000衣物/100000事件性能仍由#35验收。本票不推断推荐保存等尚未实现入口已运行，不把保存搭配/图片/试穿当穿着。ADR0018、领域术语和API schemas为下游提供一致口径。
