# Ticket #21 — 虚拟试穿与对比

实现分支 `codex/ticket-21`，独立worktree `/tmp/wardrobe-ticket-21`；基于 integration `19faa26`。F16产品范围保持完整，真实付费云效果gate待验证，不能关闭ticket。

## 实现与实际协议

- `tryon.person.import/preview`：确认本人/模特及使用权，私人PNG标准化图片；非衣物assets，不进入sync manifest/outbox。
- `tryon.generate/process`：不可变outfit版本全部商品图，人物第一张，所有衣物依次传输，Flash10张总上限（最多9件衣物），缺图/小图/超限整次拒绝。模型需明确image_generation+virtual_tryon，不接受chat/textvision通路。
- 持久化本机任务，租约/取消/前置失败重试/已提交未知核对；迟到响应不复活，不自动重新扣费。
- `tryon.preview/compare/save`：最多4方案各自前后图，provider/model/version/input provenance；结果仅本机保存，不写wear/template/clothing/sync。
- GUI在保存搭配下挂载，导入/选择私有本人或模特图片，人物预览后明确上传，方案勾选对比与本机保存，尺码/合身不作承诺。

官方能力依据与限制见 [ADR0016](../../adr/0016-private-virtual-tryon.md)。官方语义支持多图人物换衣编辑；本实现采用此图像编辑通路。配置能力、HTTP成功和fixture图不是质量证明。

## 输入与证据边界

使用仓库原创 `fictional-model.png`（96×160几何人形，CC0，无真实身份或metadata）作为人物协议输入；衣物与输出采用已有合成颜色fixture。服务器是loopback HTTP真实接口边界，不是付费Seedream推理。没有读取真实用户key、照片或数据库。

TDD首条测试先因`unsupported_operation`失败，实现person/generate后通过；第二条process/compare/save HTTP链先因未注册process失败后通过。其余针对成员/历史版本、缺图/小图/超限、重启未知与迟到取消回归。

## 本机检查

最终具体命令、计数与截图补于完成时。Chrome采用真实Vite→Rust CLI→SQLite API桥，人物导入由公开API创建；浏览器未验证Tauri原生文件选择器。320/375/414/768/1024/1280/1440检查无横向溢出、保存可达。截图显示合成协议fixture，不能解释为真实试穿效果。

## 尚未完成的验收

- 真实已开通云账户 + 明确使用权人像/模特 + 全部衣物的paid call与实际语义/保真效果；不可用loopback替代。
- Windows/macOS/Linux原生GUI文件选择器与真实云任务的平台验收。
- 不宣称人体测量、尺码建议或物理合身预测。

## 已完成的检查（合入 root `4f6152e` 后）

- `cargo test --workspace`：110 passed / 0 failed / 8 ignored（既有真实OS安全存储/实际模型下载验收按各自环境条件运行）。7个tryon核心测试与1个CLI→授权stdin MCP→HTTP→SQLite流程通过；未执行paid cloud。
- `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo build -p wardrobe-cli`、`npm --prefix apps/desktop run build`：通过。
- 首轮Chrome全GUI：31 passed / 2 skipped；新增tryon公开API+真实HTTP+两方案比较+七宽度+reload通过。最终合并后GUI计数附于下文。
- 原创人物第一张PNG经HTTP服务解码确认为96×160，随后全部两件商品图，prompt明确每件衣物与角色层次；9件+人物允许，10件+人物拒绝。
- 首次合并后链接与clippy因ENOSPC中止；仅清理独占07终态旧项目可执行文件/调试符号后重跑通过，不归类为源码失败。

命令环境：`CARGO_TARGET_DIR=/tmp/wardrobe-ticket-07/target CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 PKG_CONFIG=/opt/homebrew/opt/pkgconf/bin/pkg-config PKG_CONFIG_PATH=/tmp/wardrobe-media-native/install/lib/pkgconfig DYLD_LIBRARY_PATH=/tmp/wardrobe-media-native/install/lib`。GUI `WARDROBE_TEST_PORT=5181`、临时独立测试SQLite，测试结束无长期provider服务。target借用链接会移除，不用root/09活跃缓存。

Hallmark：复用既有Atelier tokens，组件scope Workbench，窄屏顺序前后图/宽屏并排；实际截图人工核对与七宽度assert已过。测试图是人形+红蓝合成结果，页面标签与报告明确仅协议证据。[窄屏](tryon-320.png)、[桌面](tryon-1280.png)。

最终合并后Chrome全GUI：**32 passed / 0 failed / 3 skipped**（真实OS安全存储GUI、实际抠图模型、SmolVLM服务各自环境门槛）；新tryon测试在真实Chrome/实际CLI/SQLite/loopbackHTTP重验通过。原生Tauri人物文件选择器及paid provider效果没有冒充通过。完整输出：[workspace](workspace.log)、[clippy](clippy.log)、[frontend](frontend.log)、[GUI](gui.log)。
