# wardrobe-agent · 电子衣橱 Agent

**本地优先的电子衣橱与 AI 穿搭助理。** 衣橱数据只存在你自己的设备上,AI 用你自己配置的模型额度;无账号、无订阅、无付费生成入口。

当前版本 **v0.1.0(Beta 开发中)**。本轮仅发行 macOS 桌面端;Windows/Linux 保留跨平台架构,Android 预留适配基础。

## 为什么做这个

现有开源衣橱项目各有短板:有的浏览器上传就把原图丢了,有的 API key 明文落盘,有的删除不可恢复,有的把"点了个喜欢"算成"穿过一次"。wardrobe-agent 从三个开源项目的逐行源码审计出发(见[设计参考](#设计参考)),把修正直接写进架构:

- **本地优先**:每设备独立 SQLite,免登录即可用;没有 API key 也能完成手工录入、搭配与统计的全部离线流程。
- **原图不可变**:导入先保存原图字节,抠图、压缩、商品图都是带来源追溯的派生资产;任何一步失败都不损坏原件。
- **事实与推断分离**:AI 建议、学习画像、统计投影都可以查看、修正、重置;人工确认后才写入事实。删除走回收站,历史快照永不改写。
- **一套核心,三个入口**:GUI、CLI、MCP 走同一条 Application API,输出版本化 JSON 与类型化错误,数据完全一致。

## 功能一览

### 衣橱与单品

- **多成员空间**:同一台设备为每位家庭成员建立独立衣橱,库存、历史与推荐完全隔离。
- **批量录入**:文件选择或拖拽,单批最多 100 个文件(JPEG/PNG/WebP/HEIC,单文件 ≤ 50 MiB、解码上限 60 MP);默认每张图一个草稿,可归并为同一单品。单项失败不阻断整批,重启可恢复进度、不重复入库。
- **图片工作台**:旋转、本地抠图(内置 U²-NetP 模型,离线 CPU 推理,无需任何密钥)、多图与主图管理、排序;原图只在导入设备存在。
- **AI 识别**:对接 OpenAI 兼容视觉模型,自动生成分类、颜色、材质、季节等建议;建议与事实分开存储,人工逐项纠错后才写入,重分析不覆盖人工值。
- **商品展示图**:显式启动生成任务 → 预览 → 采用;拒绝结果保留旧图,重新生成需再次确认。
- **单品管理**:自定义分类、四季多选、多值标签、收藏、归档、回收站恢复;价格用最小货币单位整数 + ISO 币种,未知就是 null,不用猜测值冒充事实。

### 搭配与推荐

- **手工搭配工作台**:任意类别自由组合(外套/裙装/鞋包配饰),角色与层次独立存储;没有 API key 也能保存搭配并生成本地拼贴预览。
- **Lookbook**:搭配模板可收藏、克隆、编辑;"今天穿""改穿"基于版本快照,改模板不改写历史。
- **AI 搭配图(flat-lay)**:用已采用的商品图由图片模型生成整身搭配图,任务保留完整输入追溯;AI 失败不丢失已保存的搭配。
- **穿搭推荐**:按场合、天气、固定与排除单品出多套方案(默认 3 套);固定项不替换,待洗与归档不进候选,近期重复默认回避 7 天。推荐只返回真实存在且可用的单品 ID,库存不足明确说明。离线时由规则评分兜底。
- **反馈分明**:喜欢/不喜欢/跳过与真实穿着分开记录,非穿着反馈不增加穿着次数。

### 穿着、洗护与洞察

- **真实穿着事件**:一次实际穿着 = 一条不可变 WearEvent,保存当时衣物快照、当地日期与 IANA 时区;支持补记、同日换装、更正与撤销,重试按幂等键去重。
- **统计口径明确**:穿着次数按事件计,穿着天数按当地日期去重,界面写清口径;未来日期不能记为已穿。
- **洗护管理**:按穿着次数阈值或手动标记提示待洗,记录清洗/干洗/维护后自动重算;补记与撤销穿着后按时间顺序重算。
- **衣橱洞察**:库存分布、穿着频率、久未穿(默认 90 天,可配置)、从未穿过、实际组合统计;全部是只读投影,可从事实事件完整复现。
- **偏好学习**:推荐反馈、真实穿着与评分成为学习信号,生成单品配对与风格画像;记忆可查看、修正、重置,不改动事实事件。

### AI 穿搭助理

- **内置 Stylist**:今日穿搭、场景与旅行计划、衣橱分析、购买适配建议;候选购买草稿不计入库存,计划不计为穿着。
- **虚拟试穿**:本人或模特图 + 已选搭配,调用具备试穿能力的图片模型,前后对比、结果仅存本地;输入数量与上限显式校验,绝不静默截断。
- **定时推荐**:按成员当地时间与星期安排;休眠错过有补执行窗口,更旧任务明确标记错过,不重复执行。
- **自带钥匙**:文本/视觉/图像生成/试穿能力按供应商声明配置,支持 OpenAI 兼容端点;凭据存入系统钥匙串,不写入普通配置、日志或同步包。

### 三个入口,一套数据

- **GUI**:React + Tauri 桌面应用。
- **CLI**:`wardrobe-cli <operation>`,版本化 JSON 输出、分页游标、稳定错误码;GUI 关闭后独立读写同一数据。
- **MCP**:`wardrobe-cli mcp` 启动,接入支持 MCP 的外部 Agent;关闭内置 AI 后,外部 Agent 仍能按能力清单读写标签、推荐与配对。
- **本机客户端授权**:外部客户端按 operation scope、成员范围和到期时间登记授权,完整授权后范围内连续调用不再逐次确认;连接令牌仅返回一次,数据库只保留 hash,与供应商 API key 分开。外部 CLI 从标准输入读取令牌:

  ```bash
  wardrobe-cli <operation> --client-stdin <<< '{ "token": "连接令牌", "input": { ... } }'
  ```

  人工维护用显式 `--trusted-local`,等同数据目录权限,不提供给外部客户端。设计与工具接入见 [ADR 0007](docs/adr/0007-local-client-authorization.md)。

### 数据所有权:同步、加密、备份

- **多后端同步**:本地文件夹、S3-compatible、WebDAV 三种后端通过同一同步契约;不同步原图与凭据,冲突保留双方版本,事件以不可变 ID 去重收敛。
- **端到端加密空间**:OS 随机空间主密钥 + 系统钥匙串解锁的 SQLite 加密钱包;设备经签名配对确认后才能读取,恢复材料单独保存。
- **备份与恢复**:一致性快照导出、恢复前差异预览、格式版本检查;是否包含原图由你显式选择。
- **健康诊断**:`wardrobe-cli doctor` 输出体检报告,`wardrobe-cli logs` 按 trace/run/operation 查询结构化日志。

## 当前状态(v0.1.0)

**分支模型(2026-10-09)**:`master` 为用户发布线(本分支),`development` 为开发集成线;开发在 development 验收后随发布提升至 master。

版本仍为 **v0.1.0**(研发试用候选,不表示正式发行)。2026-10-09 完成 MVP 发行对账(规格 [#54](https://github.com/MingStudentSE/wardrobe-agent/issues/54)):GitHub 53 张功能票全部关闭,实现分落 master 与 development 两处,逐票结论见 [docs/verification/reconciliation-20261009.md](docs/verification/reconciliation-20261009.md):

- **master 已含**:首次启动与手工录入、多成员隔离、单品管理与回收站、图片导入与压缩、可恢复批量录入、多图编辑与本地抠图、手工搭配与 Lookbook、真实穿着与改穿记录、洗护管理、衣橱洞察、AI 识别与人工纠错、商品展示图、AI 搭配图、虚拟试穿、穿搭推荐(离线规则与天气/AI)、偏好学习、加密空间与设备配对、备份与恢复、模型配置与系统钥匙串凭据、CLI/MCP 授权接入,以及 #41–#53 诊断系列(master 版实现;doctor/logs/trace、桌面诊断页)与外部客户端配对登记。
- **development 已完成并通过验收、待统一后随发布提升**:完整 Stylist、场景与旅行计划、购买适配分析、定时推荐与系统通知、本地文件夹/S3-compatible/WebDAV 三后端同步族、完整外部 Agent 契约(162 操作)与 Web 端,以及 development 版诊断实现;两线统一(master 侧治理文档与 development 侧功能合并,诊断双实现取舍)由 issue [#66](https://github.com/MingStudentSE/wardrobe-agent/issues/66) 跟踪,完成并验收前 master 不含这部分代码。
- **真实云验收已完成**:#14 商品展示图 / #20 AI 搭配图 / #21 虚拟试穿已于 2026-10-08 通过 ARK 真实云调用验收(request_id、真实耗时、token 用量与结果图 SHA256 存证齐全),报告见 [ticket-14](docs/verification/ticket-14/report-real-cloud.md)、[ticket-20](docs/verification/ticket-20/report-real-cloud.md)、[ticket-21](docs/verification/ticket-21/report-real-cloud.md)。

发行门槛现状(三平台签名缺失、逐平台验收三态与豁免、2026-10-09 数据库版本事故与防复发护栏)见 [docs/verification/release-gates-20261009.md](docs/verification/release-gates-20261009.md);F01–F30 门禁对照见 [docs/verification/ticket-35/final-gate.md](docs/verification/ticket-35/final-gate.md)。进度与收尾跟踪见 [docs/07-实施进度.md](docs/07-实施进度.md) 与 GitHub Issues。

## 从源码构建

要求:Rust(含 `cargo`)与 Node.js/npm。

```bash
# 桌面 GUI 开发会话
cd apps/desktop
npm install
npm run tauri dev

# CLI / MCP
cargo run -p wardrobe-cli -- --help

# 测试
cargo test -p wardrobe-core -p wardrobe-cli
```

> **开发注意**:开发构建、测试运行和 GUI dev 会话必须使用隔离的 `WARDROBE_DATA_DIR`。schema 迁移只向前、不可逆,用默认数据目录打开数据库会把生产库迁移到未发布版本,导致正式应用无法打开。参见 [docs/agents/build-storage.md](docs/agents/build-storage.md) 第 8 条。

## Web 端部署(内测)

桌面端与 Web 共用同一数据目录和同一套 React 界面:Web 由本机服务 `wardrobe-web` 提供,只绑定 `127.0.0.1:17838`(设计上不接受非回环绑定,局域网/远程访问需先走单独决策,见 [ADR 0029](docs/adr/0029-local-web-service.md));浏览器里看到的就是本机数据库里的数据。浏览器基线:Chrome/Edge ≥ 120、Firefox ≥ 121、Safari ≥ 17。

### 构建与打包

```bash
# 1) 服务二进制(仓库根)
cargo build --release -p wardrobe-web

# 2) 前端产物
cd apps/desktop && npm install && npm run build && cd ../..   # 产物在 apps/desktop/dist

# 3) 打包为版本化部署目录(自动维护 current 软链,产物落你指定的打包根)
scripts/web/pack-web-release.sh --worktree . --out <打包根> \
  --bin target/release/wardrobe-web --dist apps/desktop/dist \
  --cli target/release/wardrobe-cli
```

打包产出 `<打包根>/releases/v<版本>-<时间戳>/`,内含 `bin/`、`lib/`(macOS 媒体动态库)、`web-dist/`、运行脚本与 `VERSION`,并把 `<打包根>/current` 软链指向它。

### 启动与首次登录

```bash
scripts/web/start-web.sh --pack <打包根> --port 17838 --data-dir <数据目录>
```

- `--data-dir` 必须显式给出(否则拒绝启动,避免误用默认目录);与桌面/CLI 指到同一目录,就是同一份衣橱数据。
- 启动后访问 `http://127.0.0.1:17838/`;首次登录的引导令牌在 `<数据目录>/web-bootstrap-token`(权限 0600,仅限本机操作员读取)。
- 停止:`scripts/web/stop-web.sh --data-dir <数据目录>`。

### 升级与回滚

重新执行"构建与打包"得到新版本目录后:

```bash
scripts/web/upgrade-web.sh --pack <打包根> --data-dir <数据目录>                # 升级到最新打包版本
scripts/web/upgrade-web.sh --pack <打包根> --data-dir <数据目录> --rollback     # 回滚到上一版本
```

升级会切换 current 软链并重启服务,数据目录与端口不变;可加 `--backup-dir` 在升级前对数据库做一次快照拷贝。注意:数据库 schema 单向迁移,若新版本已升级过 schema 则无法回滚到旧版本(脚本会如实检测)。

### 边界须知

- 服务只监听本机回环,内测请在本机浏览器使用;会话不持久化,服务重启后需用令牌重新登录。
- 特权操作(客户端授权、模型凭据、加密空间、定时运行配置)不开放给浏览器会话,仍在桌面端或 CLI 的可信本机入口管理。
- 媒体边界与桌面一致:原图、中间图、试穿与 AI 搭配图不出本机;上传临时文件在数据目录 `web-uploads-tmp/` 下随请求结束清理。

## 设计参考

功能与交互以 `references/` 中三个开源项目的固定提交源码审计为依据([审计报告](docs/00-阅读入口与结论.md)逐条核对到源码行),吸收其验证过的设计,修正其失败模式:

| 项目 | 吸收 | 修正 |
| --- | --- | --- |
| [Wardrowbe](https://github.com/Anyesh/wardrowbe)(Next.js + FastAPI 自托管) | 完整业务闭环(录入→核对标签→推荐→穿着→洗护→反馈→学习);"规则先筛候选、模型再组合、代码校验结果"的推荐架构;反馈与实际穿着分离;单品配对学习 | 不继承 PostgreSQL/Redis/Docker 服务器依赖,改为每设备 SQLite + 本地任务调度;穿着计数从直接累加改为事件溯源重算;删除从先删图片改为回收站语义 |
| [AIWardrobe](https://github.com/leoz9/AIWardrobe)(React + FastAPI) | 上传→抠图→识别→人工纠错的短流程;批量录入单项失败不阻断整批;本地抠图与远端服务回退 | 不继承"只保存处理后结果"的原图丢失路径,改为原图先行、派生资产带来源分离;API key 从明文 JSON 改为系统钥匙串 |
| [ClosetAI](https://github.com/Ligoml/ClosetAI)(SwiftUI iOS) | 原图与衍生图分离;本地白底展示图;AI 失败回退本地拼贴;试穿前后对比 | 不继承搭配硬删除、多图输入静默截断与口径错误的久未穿统计,改为回收站、供应商输入上限显式校验、WearEvent 重算 |

三方参考中的错误穿着统计、明文密钥、原图丢失与硬删除均列为明确不可继承项;ClosetAI 仓库许可证文本缺失,本项目未复制其代码。`references/` 仅保存研究用快照,不纳入版本管理。

## 文档

- [阅读入口与研究结论](docs/00-阅读入口与结论.md)
- [需求清单](docs/01-需求清单.md)
- [Agent 功能与项目融合](docs/02-Agent功能与项目融合.md)
- [架构草案](docs/03-最终架构.md)
- [UI 设计与跨端适配](docs/04-UI设计与跨端适配.md)
- [Beta 开发规格(F01–F30)](docs/05-Beta开发规格.md)
- [领域术语表](GLOSSARY.md) · [架构决策记录(ADR)](docs/adr/)

## 许可

本项目以 [MIT 许可证](LICENSE) 开源,不设任何收费功能;自有 crate 与桌面应用(wardrobe-core / wardrobe-cli / wardrobe-desktop)均在清单中声明 MIT([#65](https://github.com/MingStudentSE/wardrobe-agent/issues/65) 已决定)。第三方依赖许可清单见 [docs/third-party-licenses.md](docs/third-party-licenses.md)(由 `scripts/generate-license-inventory.py` 从 lockfile 离线生成,可复跑校验):合计 1125 个包,其中 74 项离线缓存不可得、标识为 `unknown`,119 项非 MIT/Apache/BSD/ISC 家族列入兼容审查名单,Windows MSVC CRT 再分发条款单列。
