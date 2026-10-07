# MVP 安装包进度与发行门槛（2026-10-07）

本次仅推进 #37 三平台 MVP 安装包；#1/#35 完整 Beta 及其他未完扩展保持暂停。当前应用版本 0.1.0，仅为研发试用候选，不表示正式发行。

## 来源与验证

集成源码基线：b075fdde29190b46aa9f02e8f3f07d4751a6c6c7；当前打包修改尚在工作树。原有 macOS 安装包来源不明且曾出现数据库版本不兼容，不作为新发行包。

2026-10-07 独立核对商品图基线：product_display 6、providers 5、jobs 2、assets 9、application 10，共32通过，凭据存储1跳过；fmt通过。逐命令 exit 均为0。全部为本地协议/fixture，#14 真实供应商生成质量未验收，本次不调用云端生成。

## 声明的构建目标

| 系统 | 架构 | 格式 | 系统要求 | 签名状态 |
| --- | --- | --- | --- | --- |
| macOS | arm64 / x86_64 | APP + DMG | 构建最低13.0，13真机待验 | 无Developer ID/公证；临时签名 |
| Windows | x86_64 | NSIS EXE | WebView2及Tauri桌面运行要求，干净机待验 | Authenticode未配置 |
| Linux | x86_64 | DEB + AppImage | Ubuntu22.04/24.04分别构建，系统GUI库仍需存在 | 无发行签名 |

## 本机实际运行证据

从当前源码重新 cargo build --release -p wardrobe-cli 成功（实际exit0），经prepare-media-distribution部署私有dylib、独立CLI和许可资源。已通过移除开发loader环境后的HEIC导入、独立进程重启读取、CLI/MCP授权与一致查询；实际U²-NetP抠图、固定SHA256模型下载、网络代理不可用下缓存重启、CLI/MCP相同展示hash；通过本机Application CLI新建单品→备份→预览→另库恢复→重启读取相同记录。全部使用临时数据库，GUI恢复仍待验。这些证据不是干净OS安装验收。

详细JSON：wardrobe-execution/results/mvp-macos-distribution/{standalone-runtime,standalone-cutout-runtime,backup-runtime}.json。桌面APP/DMG已实际构建成功（exit0），APP76.91MiB，DMG23.92MiB；DMG只读挂载后包内CLI再次通过HEIC与实际抠图/离线缓存/MCP验证，挂载已释放。候选包路径为wardrobe-execution/results/mvp-macos-candidate/电子衣橱_0.1.0_aarch64.dmg，SHA256为9cc174f679d85232c8c57cb771136f2d2af7cdaff61491c0277d6b91d2eb9e8c。

原生启动通过完整新APP路径选择，实际新增PID84708指向target/release/bundle/macos/电子衣橱.app/Contents/MacOS/wardrobe-desktop；观察到已有两件单品价格199.90及299.50CNY，无数据库版本错误。没有修改现有资料；该观察只证明本机启动和读取，不证明干净安装、升级、重启写入、卸载保留。

Hallmark局部观察（Atelier应用工作台；不改设计）：侧栏导航呈“单品搭配穿着洗护”连续文字，阅读及触摸目标区分欠清晰，major；styles.css:5的inline-flex链接需在导航容器设置明确间距或逐行布局。未执行全应用审查，不能将此局部观察冒充#34验收。局部观察：0 critical / 1 major / 0 minor；Philosophy4 Hierarchy3 Execution3 Specificity4 Restraint4 Variety3。

## 外部阻断

重新读取GitHub run37559492617及check112593314320：所有job steps为空；注释明确“recent account payments have failed or your spending limit needs to be increased”。当前没有可工作的Windows/Linux runner证据。不重复触发相同阻断。恢复GitHub账单/限额或提供可用的原生runner之后，必须对冻结源码重新构建验收。

## 安装及数据说明

安装前使用应用的备份入口导出资料并核对备份预览。新版本不自动降级数据库；遇到“数据库由更新版本创建”时使用匹配版本，不删除或改写用户库。临时验证使用独立数据目录。替换APP不代表已经验证升级，卸载后数据保留、GUI重启持久与备份恢复须逐OS记录。

## 发布前仍须满足

冻结并审查源码；真实供应商商品图验收；三平台同源码产物及SHA256；产品许可证授权与完整第三方许可清单（包含MSVC CRT）；逐平台干净安装、升级、卸载数据保留、备份恢复；真实可用GUI入口。上述门槛未满足前不发布下载，不关闭#37。

## 打包流程审查

media-bundles.yml现含Windows/Ubuntu22.04/24.04和macOS14 arm64/macOS15 Intel。独立macOS原生库及CLI staging生成APP+DMG overlay；实际DMG只读挂载后验证CLI，退出trap释放挂载；portable tar保留执行位。全部平台对实际安装格式生成SHA256SUMS。root核对YAML、全部Bash代码块bash -n及校验和Python编译，git diff --check通过。未运行受阻CI，不将语法检查当作跨平台执行证据。

GLM Flash完成workflow主体编辑与Bash语法检查后，准备额外模拟制品测试；root已有真实DMG证据，停止该重复步骤（wrapper exit130），随后独立审查并补全跨平台校验和。该wrapper不是通过证据。修改仍为未提交工作树，没有推送或触发CI。

随候选产物提供依赖许可盘点：393个workspace解析Rust包、88个npm锁定包；这是元数据清单，非完整许可批准。desktop独立依赖与MSVC CRT等仍须补全，未自行选择产品开源许可证。

## 用户最新调整：全部本地构建

用户明确不再使用Actions，三平台包改为本地构建；Windows只要求产出EXE，不做安装/运行验收。新加的Actions扩展已撤回并保存外部patch，仓库原有workflow保留，没有触发。GitHub账单不再是本次构建前置条件。

本地环境：Homebrew Lima2.2.1/QEMU，Ubuntu22.04 x86_64 guest；LLVM23.1.2和LLD23.1.2外置工具链、cargo-xwin0.23.1、x86_64-pc-windows-msvc target、NSIS3.13。工具与镜像来自官方发行/包注册表。构建卷位于备份盘内新建HFS+ sparsebundle；首次APFS镜像挂载失败，没有使用或改动其他磁盘。新建Lima缓存迁到构建卷，系统默认位置保留symlink。
