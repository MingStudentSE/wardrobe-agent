# 电子衣橱视觉系统

2026-10-07 首次锁定。Audience：本地衣橱拥有者；Use case：浏览与手工录入已有单品；Tone：安静、编辑感。Hallmark 用于视觉纪律；产品使用工作台结构，不出现营销 hero 或页脚。

共享源：`apps/desktop/src/styles.css` 命名 tokens。Atelier 方向：暖白纸面、深绿行动色、克制细边框；中文标题采用本机宋体回退，正文系统中文无衬线字体。离线不拉取字体。不得逐页换主题。单品占位图明确标识“暂无图片”。

导航桌面侧栏、窄屏头部；衣物区与录入区宽屏双栏、窄屏顺序单列。按钮与表单至少 48px，关键操作始终可见。季节使用原生 checkbox，错误带文字与 role=alert，成功带文字与 role=status。焦点轮廓独立颜色，减少动画偏好保留；不依赖 hover 或拖拽。

默认、hover、focus-visible、pressed、disabled、loading、error、success 均实现。输入 active 使用 focus，禁用仅用于提交不可用；只读/非交互单品卡不伪装成按钮。原生输入校验与 API 错误共同提供反馈。

后续页面遵守相同 tokens、布局与状态语义；新增角色色必须先进入 token block。响应式验收宽度：320/375/414/768/1024/1280/1440。CSS 实现不等于这些视口已实测通过。
