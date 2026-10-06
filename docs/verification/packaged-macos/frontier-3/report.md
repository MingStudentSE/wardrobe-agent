# Frontier 3 macOS package verification

Source7d783db，macOS26.6.2/aarch64。实际重新编译release CLI、44模块React前端及Tauri .app（27.07MiB），包含批量录入、Lookbook与显式视觉模型验证。独立私有libheif/libde265及sidecar CLI随包打入。

对portable CLI和实际.app/Contents/MacOS/wardrobe-cli分别运行verify-packaged-cli.py，移除开发加载环境后，限定客户端stdin授权、无授权CLI/MCP拒绝、HEIC64×32导入、退出重启读取和MCP查询均通过。原始JSON同目录保留。这些烟测验证媒体/授权运行边界，不冒充完整批量/搭配功能测试；后者见ticket06/08真实Application与GUI报告。

原生GUI启动、系统文件选择、macOS13安装及全Beta验收仍待完成；构建成功不等于这些门槛通过。
