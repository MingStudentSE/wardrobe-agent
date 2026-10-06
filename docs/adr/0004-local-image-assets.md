# ADR 0004：本地原图与规范化展示资产

状态：采用实现方案；20 张实物质量对比已通过；完整GUI与跨平台发布待完成（#5）。

Application API 提供 assets.import/get/preview/rebuild_thumbnail/adopt，三个入口共享实现。原图字节先存 SHA-256 CAS 的 originals-local-only 区，随后解码；失败不建立展示资产、不改变单品主图。原文件始终保留。资产记录 ID 为 UUIDv7，展示 blob 另以处理后字节 hash 标识，原图 hash 和路径只存在本地记录。每单品最多 20 个图片引用，采用使用同一 IMMEDIATE 事务修改资产与单品、audit/outbox，outbox 不包含原图路径、文件名或 EXIF。

文件上限 50 MiB，JPEG/PNG/WebP 的头部尺寸及 HEIF primary handle 和 ispe 尺寸在完整解码前检查 60 MP 上限。image decoder 读取 EXIF 方向，libheif 默认应用 HEIF 容器方向；后者不二次套用 EXIF。解码后重编码去除元数据。展示最长边 1600，不放大，WebP 质量 82，透明通道保留。500 KiB 为软阈值，超出时提示检查并允许保留细节；当前不会为体积单独降低质量，20 张实物对比未发现不可接受损伤，保留 q82，证据见 docs/verification/ticket-05-quality/report.md。缩略图最长边 320，源为展示图，原图缺失也可重建。

libheif-rs 3.0 (MIT) 使用 libheif 最低 1.19 API（上下文解码限额）；独立构建脚本固定 libheif 1.23.1 与 libde265 1.1.3，解码库动态链接，禁用 GPL x265/x264 编码器。libheif / libde265 为 LGPL；发布必须提供相应许可、源代码链接及动态库替换机制。macOS/Windows/Linux 安装包仍需要部署库和依赖、签名与真实机器解码验证，Homebrew 开发机成功不等于发布通过。Windows vcpkg 固定 2026.07.29，libheif[core] 包含必需 libde265，避免默认 hevc 编码器功能引入 GPL x265。

来源：[libheif](https://github.com/strukturag/libheif/tree/v1.23.1)、[libde265](https://github.com/strukturag/libde265/tree/v1.1.3)、[vcpkg port](https://github.com/microsoft/vcpkg/blob/2026.07.29/ports/libheif/vcpkg.json)、[Rust wrapper](https://github.com/Cykooz/libheif-rs)。自动化生成的边界 fixture 仅验证算法，不计入实物服装质量验收。
