# 正式 React 界面验证

2026-10-07。源码接入分支：codex/ui-light-blue。

`npm run build --prefix apps/desktop`：TypeScript + Vite生产构建通过。
`DYLD_LIBRARY_PATH=/Users/links/.codex/wardrobe-execution/native-media/install/lib npm run test:e2e --prefix apps/desktop -- redesign.spec.ts`：真实CLI/Application API+隔离临时SQLite验证。没有以mock代替持久化。

截图：七种宽度的库存网格（测试种子）；with-real-images-1280.png和details-with-image-1280.png为隔离数据库中的五件原型参考衣物，图片经过真实assets.import/adopt后呈现。

范围：四工作区页面隔离；表单草稿跨页；单品创建后重新打开；只读详情与编辑分离；关闭草稿保护；分页位置；弹窗焦点与Escape；320/375/414/768/1024/1280/1440共28次页面溢出检查。

不代表Tauri安装包/三平台/所有Beta功能终验。旧测试中默认首页存在所有表单的定位方式需逐项迁移，本次没有声称旧全量测试套件通过。
