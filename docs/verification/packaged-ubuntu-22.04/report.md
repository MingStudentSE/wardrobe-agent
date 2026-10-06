# Ubuntu 22.04 packaged media runtime

GitHub run37508783217，source9b30572，bundle(ubuntu-22.04)整体success。实际生成deb/AppImage，下载artifact后核对三份runtime JSON和GUI ldd输出。

portable、deb解包与AppImage解包路径的独立CLI，均在移除开发加载环境后通过限定operation/member客户端stdin授权、无授权CLI/MCP拒绝、HEIC64×32导入、退出重启读取与授权MCP查询。deb GUI从包内私有目录加载libheif.so.1/libde265.so.0，ldd无not found。

本证据仅覆盖实际包内CLI和依赖加载；桌面GUI启动、系统安装/卸载/升级与完整Beta验收仍pending。后续#6/#8及视觉验证不在本次source内，新版本工作流另行验证。原始JSON/loader输出保留同目录。
