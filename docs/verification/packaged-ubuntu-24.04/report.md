# Ubuntu 24.04 packaged media runtime

GitHub run37508783217，source9b30572，bundle(ubuntu-24.04)整体success。实际生成deb与AppImage，下载其上传artifact后逐份核对原始JSON与GUI动态加载器输出。

独立portable、deb解包路径与AppImage解包路径的CLI均移除开发加载环境后通过：限定operation/member客户端stdin授权、无授权CLI/MCP拒绝、HEIC64×32导入、退出后重启读取、授权MCP查询。deb GUI的ldd从包内私有目录加载libheif.so.1/libde265.so.0，无not found。三份runtime JSON与loader输出同目录保留。

这是实际包内CLI及依赖加载证据，不代表桌面GUI实际启动、系统安装/卸载/升级或完整Beta验收；gui_clean_install明确pending。Windows与Ubuntu22.04仍在同一工作流运行。后续模型视觉验证与#6–#8代码不在本次source内。
