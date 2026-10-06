# HEIC release ABI 修复实测（2026-10-07）

机器：macOS arm64；原生 decoder 使用 isolated prefix 的 libheif 1.23.1 与 libde265 1.1.3，已以 macOS 13 deployment target 构建。测试输入为项目自制 64×32 HEIC fixture；它验证解码与发行运行路径，不另计入 20 张实物质量样本。

## 真实 red 与根因

同一 native prefix 下，`cargo test -p wardrobe-core --test assets heic_decodes_real_container` 调试通过；`cargo test --release -p wardrobe-core --test assets heic_decodes_real_container` 实际失败 `image_decode_failed`。原始 release CLI 与 portable CLI 均失败，因此不是仅有打包漏库。

临时 native/Rust 诊断显示 codec 注册正常：`libde265 HEVC decoder, version 1.1.3`。native header/API 返回 `security_version=4, security_size=104`；Rust v1_19 绑定的 SecurityLimits 为64字节。wrapper `security_limits()` 按值复制64字节却保留 version4，native `copy_security_limits` 按 version>=2/3/4 读取后续字段，造成对象外读取。release 诊断 `read_bytes` 错误为 `InvalidInput / NoFtypBox`，尽管 fixture 相同且正确；调试栈布局暂时掩盖错误，不能视为兼容证据。

源码依据：[native限额结构开头的uint8_t version与version4扩展](https://github.com/strukturag/libheif/blob/v1.23.1/libheif/api/libheif/heif_security.h)、[native按版本复制扩展字段](https://github.com/strukturag/libheif/blob/v1.23.1/libheif/context.cc)、[wrapper按值复制与私有inner](https://docs.rs/libheif-rs/3.0.0/src/libheif_rs/security_limits.rs.html)。这些内容与本机固定tag源和crates.io3.0.0源码逐项核对；首字节检查基于明确的C结构定义，不依赖Rust私有结构内存布局。

## 修复与真实 green

绑定固定 libheif-rs3.0.0/libheif-sys5.3.1，启用 v1_23，104字节结构与随包 native 匹配。初始化 guard 保持到所有 native 对象处理完成。运行时检查 libheif1.23.x，且仅读取 native 全局结构第一字节验证 version4，再调用 wrapper 复制结构；未知 ABI 返回 unsupported_heic_runtime，不解释未知扩展字段。wrapper 没有公开 version getter，所以该1字节检查采用固定sys getter，不将完整 native 指针按未知结构解引用。

未修改60MP与256MiB block限制。修复后临时诊断 `read=Ok, decode=Ok(64,32)`；Application API 的9个 release资产测试全部通过。CI 增加现有 HEIC 行为测试的 release profile 执行，避免再次只以debug测试判断native兼容。

实际 CLI `MACOSX_DEPLOYMENT_TARGET=13.0 cargo build --release -p wardrobe-cli`，再使用 `prepare-media-distribution.py` 复制原生库、相对rpath并adhoc签名。`verify-packaged-cli.py` 清除开发loader变量与开发PATH后实际通过：capabilities29项、HEIC64×32、重启获取相同记录、独立stdio MCP查询相同记录。`vtool` 检查portable CLI的 minos=13.0。运行报告位于 `/tmp/wardrobe-heic-fixed-distribution-final/standalone-runtime.json`，发行预览目录为 `/tmp/wardrobe-heic-fixed-distribution-final/portable/`。

该实测不是macOS13真机运行、正式签名公证或Windows/Linux发行验收；后续平台CI与#35门槛继续单独验证。临时诊断example已移除，原始fixture未改写。
