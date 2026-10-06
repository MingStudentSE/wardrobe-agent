# 图片界面接入验证

2026-10-07，macOS，Chrome 浏览器。单独工作树 codex/media-ui。项目固定 Hallmark Atelier 系统；未修改工作流，CI 与原生安装由集成负责人验收。

## 已验证

- 真实 PNG/HEIC 经 CLI/Application API 解码、压缩、生成缩略图；前端只替换系统文件选择器外部边界，不替换任何业务 API。
- 新图片先预览，显式采用追加关联并切换主图；既有图片引用保留。衣物编辑不丢图片；重新打开后仍可读取。
- 按钮排序、详情内确认移除关联、主图替补、其他衣物引用及本地原图字节保留。最后关联移除后图片回到 preview/local_only。
- 逐项解码超限错误及重试与有效预览独立。读取展示图失败提供重新读取，不使用原图兜底。
- 系统文件选择等待时阻止成员切换与并发保存；原有成员切换、分页并发与精确金额测试全部保留并通过。
- 320/375/414/768/1024/1280/1440 宽度无页面横向溢出，图片控件可见。截图为 `/tmp/wardrobe-media-ui-{width}.png`（完整页面）与 `/tmp/wardrobe-media-detail-{width}.png`（图片详情区域）。已人工查看 320、1280 图片详情截图。

## 命令与结果

本机原生媒体依赖环境：PKG_CONFIG=/opt/homebrew/opt/pkgconf/bin/pkg-config；PKG_CONFIG_PATH=/tmp/wardrobe-media-native/install/lib/pkgconfig；DYLD_LIBRARY_PATH=/tmp/wardrobe-media-native/install/lib。

- `cargo test --workspace`：40 通过、1 忽略（OS 凭据库烟测，非图片测试）；已合并 provider 与旧连接 schema 保护测试。
- `cargo clippy --workspace -- -D warnings`：合并新增 provider 后通过。
- `npm run build --prefix apps/desktop`：通过。
- `WARDROBE_TEST_PORT=5178 npm run test:e2e --prefix apps/desktop`：15 通过，图片用例 4 项。

## 边界

浏览器夹具不能替代真实原生文件选择、三平台安装或至少 20 张衣物样图质量验收。原生打包和样图检查仍由 #5 整体验收记录覆盖。未来穿着事件如引用图片，资产同步状态的引用计数需同时纳入不可变历史引用；当前文件与 audit 快照不会被移除关联删除。
