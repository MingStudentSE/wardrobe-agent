# 衣橱重设计 · 可交互原型

设计系统和操作流程见根目录 `design.md`。设计任务：https://github.com/MingStudentSE/wardrobe-agent/issues/38

打开 `preview/index.html` 可直接使用；或者在仓库根目录运行：

```sh
python3 -m http.server 8765 --bind 127.0.0.1 --directory design/preview
```

访问 http://127.0.0.1:8765 。无安装、无网络依赖、无后端。真实衣物图片来自用户指定插件模板目录的复制，原图未改动。

## 验证

`verify.cjs` 使用本机 Chrome 和已配置的 Playwright runtime 检查当前原型。需要先启动预览服务。

- 四工作区 × 七宽度 = 28个页面溢出检查。
- 弹窗窄屏边界；搜索、品类筛选、新增必填校验与无图单品；详情焦点恢复；穿搭保存；实际穿着记录与同日去重；刷新复原。
- 空、加载、错误、重试、离线、长名称；减少动画；200%浏览器缩放的375px溢出检查。
- 截图：`verification/`。原型检测结果：`verification/results.json`。

浅色原型已检查。未进行深色、Android真机、三平台Tauri运行、持久化/真实API、生成模型或同步后端验收。状态切换为模拟，非网络故障注入。初次验证的状态遍历脚本曾因重复关闭details超时，已修复并重跑；没有把该超时报告为产品通过。

Hallmark审查：保留真实照片、命名tokens、无营销hero/渐变/伪造统计、主次层级、当前页导航、48px触摸目标、键盘焦点和减少动画。营销/主题轮换门槛不适用于本应用设计；同一系统内页面保持一致。当前原型不是完整Beta验收。

参考衣物图片与截图仅保留在本机，不随正式源码提交。重新检出时可自行放入 `preview/assets/`；正式React界面通过真实资产API读取用户本地衣物，不依赖这些参考文件。
