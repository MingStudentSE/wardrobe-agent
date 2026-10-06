# HEIC 原生构建证据

2026-10-07。隔离原生前缀 /tmp/wardrobe-media-native/install，libheif v1.23.1、libde265 v1.1.3，仅 HEVC 解码器，无 x265 编码器。

bootstrap-media-unix.sh 实际构建成功。修正 macOS 部署目标后再次构建，vtool -show-build 对两库均显示 minos 13.0。Tauri 原生 release 编译通过；prepare-macos-media-config.py 生成 frameworks 配置后 .app 打包成功（15.75 MiB），Contents/Frameworks 包含 libheif.1.dylib 与 libde265.0.dylib，程序链接 @rpath/libheif.1.dylib。

上述打包首先发生在部署目标修正之前；修正后的最终包重新构建及独立运行待复验。构建成功不证明 macOS 13 实机安装、Windows/Linux 安装或20张实物压缩质量验收。

跨平台 CI 已增加 Unix 隔离 bootstrap 与 Windows vcpkg 解码器 bootstrap；对应运行37504689956当前pending，未判通过。
