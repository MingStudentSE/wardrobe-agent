# macOS application and independent CLI runtime

2026-10-07，macOS26.6.2/aarch64，source0fee47f。完整React/Tauri .app（26.53 MiB）包含模型设置、客户端授权、图片界面、独立CLI、解码器与许可证；vtool确认主程序minos13.0。

实际执行prepare-media-distribution.py生成私有库和framework配置，再tauri build打包。分别对portable CLI及实际.app/Contents/MacOS内CLI执行verify-packaged-cli.py，移除DYLD/开发PATH/pkg-config/vcpkg相关变量，独立临时SQLite上完成一次限定客户端登记、无token CLI/MCP拒绝、stdin授权、HEIC64x32导入、退出后重新读取、MCP等价查询。两份JSON为实际通过结果，无原始令牌或供应商key。

原生GUI启动和系统文件选择仍被Mac锁屏阻止；此记录不能替代macOS13实机、Windows/Linux安装及完整Beta验收。真实供应商账户调用仍未验证。
