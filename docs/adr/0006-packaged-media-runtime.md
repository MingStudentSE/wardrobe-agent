# ADR 0006：随发行包部署媒体解码库与独立 CLI

状态：实现打包路径；Windows/Linux 实际 installer CI 与干净安装验收待运行（#5/#35）。

不依赖开发机的 vcpkg / isolated CMake prefix 或 LD_LIBRARY_PATH。`prepare-media-distribution.py` 复制已构建的原生 release CLI 和动态库，生成 Tauri overlay；不修改业务、认证或媒体 UI。GUI 包内使用 externalBin 部署独立 `wardrobe-cli`，用户可直接以 `wardrobe-cli mcp` 启动 stdio MCP，GUI 无须打开。

Windows NSIS 安装目录同时包含 GUI、CLI、libheif/libde265 及其 vcpkg DLL 和 licensed MSVC app-local x64 CRT。使用 bundle.resources 的源文件到根目录映射。独立 CLI 目录采用相同平面布局。vcpkg core 避免 GPL x265 编码器；vcpkg copyright 文件随包复制。Microsoft CRT 再分发条款和全部发行依赖仍须 #35 发布许可审计确认，不能将 CI runner 的已安装 CRT 当成干净 Windows 的保证。

Linux `.deb` 与 AppImage 中两个入口均位于 usr/bin，媒体库放在 usr/lib/wardrobe-agent。ELF RUNPATH 为 `$ORIGIN/lib:$ORIGIN/../lib/wardrobe-agent`；独立 CLI 的私有库放在同目录的 lib，库自身 RUNPATH 为 `$ORIGIN`。Tauri Linux custom files 明确复制两个 SONAME 文件，不从系统的旧 libheif 解码器取代私有版本。Debian/AppImage 仍依赖其各自桌面系统基础库，不声称独立 CLI 是静态无依赖程序。CLI 与 installer tar 归档保留 Linux 可执行权限。

专用 `Packaged media runtime` workflow 支持 workflow_dispatch，并在 integration 分支打包脚本变更时自动运行；没有与既有编译 CI 共用的 concurrency group。实际构建 NSIS/deb/AppImage，执行 standalone CLI、NSIS 安装后的 CLI、解包 deb/AppImage 内 CLI。`verify-packaged-cli.py` 移除开发 loader/path 变量，在新临时数据库实际执行 HEIC 导入、重启获取和 MCP 等价查询。它是行为 smoke，不以静态配置断言替代运行证据。提取 deb 另检查 GUI 的 loader 解析到私有 libheif 且无缺失库。全部 runtime 报告随 artifact 上传。

构建、解包或 CI runner 安装成功都不等于 #35 完成：仍需各目标干净 OS 的 GUI 启动/升级/卸载保留数据、签名、备份恢复等实测。macOS 的 framework overlay 与最低 macOS 13 构建由现有流程验证，本决策不覆盖或改变它。

官方依据：[Tauri externalBin](https://v2.tauri.app/reference/config/#bundleconfig)、[资源映射](https://v2.tauri.app/develop/resources/)、[Linux AppImage](https://v2.tauri.app/distribute/appimage/)、[配置中的 Linux custom files](https://v2.tauri.app/reference/config/#linuxconfig)。
