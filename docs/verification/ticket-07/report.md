# #7 多图编辑与本地抠图验证

实现 F06/F26：最多20图的追加、排序、主图、跨单品图片归并、旋转、替换、恢复和本地模型抠图。旋转/恢复/抠图均生成新资产与派生来源，保留原始文件、旧展示图和搭配版本引用；预览只有明确采用才改变单品事实。GUI、CLI、MCP 均调用真实 Application。

## 可重跑验证

- `cargo test --workspace`：全部默认 Rust 测试通过；真实下载测试必须显式执行，未把忽略测试当成通过。
- `cargo clippy --workspace --all-targets -- -D warnings` 和 `cargo fmt --all --check`。
- `cd apps/desktop && npm run build && npm run test:e2e`：24项正常编辑、归并及既有全应用流程通过（31.7秒），1项需网络的真实模型烟测在该默认运行中跳过、已另行显式通过；公开模型 GUI 烟测按下方环境显式启用。
- `WARDROBE_CUTOUT_PUBLIC_SAMPLE="$(pwd)/crates/wardrobe-core/tests/fixtures/OKM_Shirt_Front.jpg" cargo test --release -p wardrobe-core --test image_editing real_model_download_white_shirt_cutout_and_cached_offline_restart -- --ignored --nocapture`：本机成功。官方权重真实下载、校验，白衣1200×1600抠图，地面透明而衣物中部不透明，重启复用缓存，源图字节保持一致。
- `WARDROBE_RUN_CUTOUT_GUI_SMOKE=1 npm run test:e2e -- --grep '公开白衣'`：实际 GUI 下载、抠图、展示预览、明确替换、重新载入显示离线缓存通过（30.9秒）。截图 `actual-white-shirt-cutout.png` 已人工查看。
- `python3 scripts/verify-cutout-cli.py --help` 给出可重跑发布包烟测参数。本机 relocated release CLI 移除开发 loader 环境后，通过官方获取、独立 CLI 与全新 MCP 进程推理；获取完成后 HTTP 代理不可用仍成功复用缓存。具体结果见 `macos-relocated-cutout-runtime.json`。

TDD 记录：首先增加缺少 images 操作的契约测试得到 unsupported_operation；实现后旋转/恢复/归并/缺失模型持久失败测试变绿。真实 GUI 归并测试暴露错误 search 字段及来源选择器定位，修复后通过。完整GUI回归还暴露新增测试对共享夹具的污染（剩余图片和单品影响既有计数），补充明确测试清理后24项回归全部通过。后续增加最大20图原子拒绝、跨成员拒绝及 CLI/MCP 持久取消一致性测试通过。取消与租约由真实 #6 持久 jobs 实现；下载故障最多3次自动尝试、读取 Retry-After，永久错误需要明确重试。

## 真实来源与质量边界

原始照片 `OKM_Shirt_Front.jpg` 来自 Wikimedia Commons，Prerobnem，CC BY-SA 4.0；原始哈希、出处与完整许可证见 fixtures 对应 LICENSE 和本目录 public-photo-source.json。截图所含处理后照片继续按 CC BY-SA 4.0 提供，标注为本项目抠图派生。此验证未访问用户衣橱照片或供应商密钥。

模型为官方 rembg release U²-NetP，4,574,861字节，SHA256 `309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8`；CPU runtime tract-onnx 0.23.8。权重作者明确 Apache-2.0 授权证据、模型出处和完整许可证随包提供，详见 ADR0009 与 third-party。DUTS 与 ImageNet 数据库条款存在研究/教育范围限制；本项目不访问或分发这些训练数据，未把代码许可证或数据库页面未声明模型限制解释为商业模型许可结论。免费 Beta 范围继续验证，商业传播影响仍待终审。

真实白衣被保留，背景大部分透明；边缘存在少量棕色光晕和残留背景，因此 GUI 明确提示核对结果、可以丢弃预览，旧图不因失败消失。此模型并不保证所有服装与背景质量。

## 发布验收界限

五平台 CI 已添加显式真实权重下载/校验/服装推理与发布包 CLI/MCP 烟测（macOS Intel/ARM、Windows、Ubuntu22.04/24.04），网络失败会失败，不能用仅编译代替。此本机报告只证明 macOS ARM 发布 CLI 和开发 GUI 实际路径；其余平台权威 CI、各平台安装后原生 GUI 验收仍待根任务汇总，不能据此关闭票据。

合并 #6/#8 后独立复验：workspace 76 passed / 2 ignored，fmt/clippy/build通过；完整GUI24 passed、1显式模型GUI烟测默认跳过。另行真实release模型下载/校验/白衣推理/离线重启测试已通过。报告原来的相对样本路径在Cargo测试工作目录报NotFound，以上重跑命令改为绝对路径。实际release CLI重新staging到独立portable目录，去开发loader环境后官方权重获取、CLI与全新MCP推理、禁用HTTP代理后的缓存重启、源图不变均通过；证据 merger-macos-cutout-runtime.json。DB最终max8、jobs/imports/outfits迁移与历史资产引用保留；未预设后续wear_asset_refs存在。跨平台实际推理与各平台原生GUI仍待权威验证。
