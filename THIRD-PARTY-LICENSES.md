# 第三方许可清单（THIRD-PARTY-LICENSES）

发行基线：分支 `codex/ticket-35-release`（起点 `codex/beta-integration-continue` tip `464a845`），macOS aarch64 产物。生成方法与原始行数据见 `docs/verification/ticket-35/`（`cargo-license-rows.json`、`npm-license-rows.json`、`cargo-metadata.json`）。完整逐项表格见本目录 `docs/verification/ticket-35/license-appendix.md`。

## 1. 本产品许可证：待定（未选择）

**截至本清单生成时，仓库没有 LICENSE 文件，workspace 各 crate 的 `license` 字段均为空。** 产品开源许可证（含与下述 LGPL 动态库、Apache-2.0 模型权重的兼容性）须由项目所有者决定后在此处补齐；本次发行材料如实将其列为未决项，不代为选择。

## 2. 原生媒体库（随 macOS 包分发，动态链接）

| 组件 | 版本 | 许可证 | 分发方式 | 义务履行 |
| --- | --- | --- | --- | --- |
| libheif | 1.23.1 | LGPL-3.0-or-later（library）；示例/封装 MIT | 动态库 `libheif.1.dylib` 装入 `电子衣橱.app/Contents/Frameworks` 与独立 CLI `lib/`，可替换重链 | COPYING 随包分发于 `Contents/Resources/licenses/` 与 portable `licenses/`；源码地址见 portable `README.txt`（https://github.com/strukturag/libheif/tree/v1.23.1） |
| libde265 | 1.1.3 | LGPL-3.0-or-later（library）；示例 MIT | 动态库 `libde265.0.dylib`，同上可替换 | 同上（https://github.com/strukturag/libde265/tree/v1.1.3） |

两个库均为 LGPL，采用动态链接 + 独立可替换 dylib + 随包许可证文本 + 上游源码链接的方式履行义务；未静态链接任何 GPL/LGPL 代码进 Rust 二进制。

## 3. 本地抠图模型权重

| 组件 | 许可证 | 来源与校验 |
| --- | --- | --- |
| U²-NetP（u2netp.onnx） | Apache-2.0（作者在 issue #208 明确确认代码与模型许可） | 下载源 https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx ，固定 SHA256 `309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8` 校验后缓存，不随安装包内置；完整 NOTICE 见 `third-party/U-2-Net-NOTICE.md` |
| tract（本地 ONNX 推理运行时） | Apache-2.0 OR MIT | `third-party/tract-NOTICE.md`、`third-party/licenses/` |

## 4. Rust 依赖（Cargo.lock 运行时闭包）

- 解析到 **380 个** crate（normal 依赖闭包，含跨平台 target-specific 依赖）。许可证分布：MIT/Apache-2.0 系（含双许可表达）约 97%；其余为 Unicode-3.0、BSD-2/3-Clause、ISC、Zlib、0BSD、BlueOak-1.0.0 等宽松许可。
- **需单独标注项**：
  - `webpki-roots 1.0.9` — **CDLA-Permissive-2.0**（Mozilla 根证书数据集）。宽松数据许可，允许再分发；非 OSI 传统开源许可，特此标注。
  - `r-efi 6.0.0` — 许可含 `MIT OR Apache-2.0 OR LGPL-2.1-or-later` 选项，但其依赖目标为 `cfg(target_os = "uefi", ...)`，**不参与 macOS（aarch64-apple-darwin）编译**，不在产物内。
- 无 GPL/AGPL 运行时依赖；除上述标注外无 Copyleft 项。
- 逐项 crate/版本/许可证见附录 A（`docs/verification/ticket-35/license-appendix.md`）。

## 5. 前端依赖（apps/desktop，npm）

- 共 **28 个**包（含构建期工具）；进入桌面产物 bundle 的运行时依赖仅 `react`、`react-dom`、`@tauri-apps/api`、`@tauri-apps/plugin-dialog`（及其传递依赖 `scheduler`、`csstype`）。
- 许可证分布：MIT 18、Apache-2.0 4、Apache-2.0 OR MIT 3、ISC 1、BSD-3-Clause 1、MIT OR Apache-2.0 1。**全部宽松许可，无 Copyleft。**
- 桌面运行时渲染使用 macOS 系统 WebView（WKWebView），不随包分发任何浏览器引擎。
- 逐项见附录 B。

## 6. Copyleft / 冲突项汇总

| 项 | 许可证 | 处置 |
| --- | --- | --- |
| libheif / libde265 | LGPL-3.0-or-later | 动态链接 + 可替换 dylib + 随包 COPYING + 源码链接（已履行；若未来静态链接须重新评估） |
| r-efi | MIT/Apache-2.0/LGPL-2.1+ 三选 | 不在 macOS 编译闭包 |
| webpki-roots | CDLA-Permissive-2.0 | 宽松，随 rustls/reqwest 进入产物，已标注 |
| 本产品自身许可证 | 未选择 | 见第 1 节，发布前须由所有者决定 |

## 7. 生成方式复核记录

- Rust：`cargo metadata --format-version 1 --locked`（exit 0），自 `workspace_members` 沿 normal 依赖闭包遍历；许可证取各包 `license` 字段。
- npm：扫描 `apps/desktop/node_modules`（npm ci 后）全部 `package.json`（浅层优先）。
- 原生库与模型：读取 `native-media/libheif/COPYING`、`native-media/libde265/COPYING`、`third-party/U-2-Net-NOTICE.md`、`third-party/licenses/`。
