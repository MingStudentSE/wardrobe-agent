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
