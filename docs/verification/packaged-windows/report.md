# Windows packaged media runtime

GitHub run37508783217，source9b30572。Windows实际Tauri NSIS构建、安装到隔离目录、portable与已安装CLI烟测步骤均success；下载已上传artifact并逐份核对原始runtime JSON。记录时job仍保存Rust缓存，不提前宣称整体工作流终态。

两份实际CLI均在移除开发加载环境后，通过限定operation/member客户端stdin授权、无授权CLI/MCP拒绝、HEIC64×32导入、退出重启读取与授权MCP查询。解码DLL与app-local MSVC CRT随包配置；没有依赖开发PATH兜底。

实际NSIS安装+CLI证据不等于GUI启动、Windows10最低版本、卸载/升级或完整Beta验收；gui_clean_install明确pending。后续#6/#8与显式视觉验证不在本次source内，新版本另行验证。两份原始JSON保留同目录。
