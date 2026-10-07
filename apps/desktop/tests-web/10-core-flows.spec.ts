import {test,expect} from '@playwright/test';
import {fileURLToPath} from 'node:url';

// Web 核心流程（真实照片、真实持久化、真实会话）：成员、单品录入、手工搭配、
// 穿着、洞察、离线推荐、批量录入、设置能力边界、窄屏与键盘。
const photo=fileURLToPath(new URL('../../../crates/wardrobe-core/src/validation-image.png',import.meta.url));

test('web core flows work end to end with real photos and persistence',async({page})=>{
 await page.goto('/');
 await expect(page.getByRole('heading',{name:'我的衣橱'})).toBeVisible();

 // ---- 成员管理：创建独立成员并来回切换 ----
 // 侧栏成员表单在 900px 高度会被 100dvh 侧栏裁剪（桌面 #34 已知问题），用高窗口操作。
 await page.setViewportSize({width:1280,height:1600});
 await page.getByText('当前衣橱',{exact:true}).click();
 await page.getByText('管理成员档案',{exact:true}).click();
 await page.getByLabel('新成员名称').fill('妹妹');
 await page.getByRole('button',{name:'创建成员',exact:true}).click();
 await expect(page.getByRole('combobox',{name:/^当前成员/})).toHaveValue(/.+/);
 const memberCombo=page.getByRole('combobox',{name:/^当前成员/});
 await memberCombo.selectOption({label:'我'});
 await expect(memberCombo).toHaveValue(/.+/);

 // ---- 单品录入：真实照片导入 → 压缩 → 展示 ----
 await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();
 const dialog=page.getByRole('dialog');
 await dialog.getByRole('region',{name:'录入单品'}).locator('input[type=file]').setInputFiles(photo);
 await expect(dialog.getByText('照片已准备好')).toBeVisible();
 await expect(dialog.getByRole('img',{name:'待添加的衣物照片'})).toBeVisible();
 const itemName='Web 端验收夹克';
 await dialog.getByLabel('名称',{exact:true}).fill(itemName);
 await dialog.getByRole('button',{name:'添加到衣橱',exact:true}).click();
 await expect(dialog).not.toBeVisible();
 const cards=page.locator('.collection .grid article');
 await page.getByLabel('搜索衣物',{exact:true}).first().fill(itemName);
 await page.getByRole('button',{name:'搜索',exact:true}).click();
 await expect(cards).toHaveCount(1);
 await expect(cards.locator('img')).toBeVisible();

 // ---- 重载恢复：持久数据在页面刷新后仍在 ----
 await page.reload();
 await page.getByLabel('搜索衣物',{exact:true}).first().fill(itemName);
 await page.getByRole('button',{name:'搜索',exact:true}).click();
 await expect(page.locator('.collection .grid article')).toHaveCount(1);

 // ---- 成员隔离：妹妹的衣橱是空的 ----
 await page.getByText('当前衣橱',{exact:true}).click();
 await memberCombo.selectOption({label:'妹妹'});
 await expect(page.locator('.collection .grid article')).toHaveCount(0);
 await memberCombo.selectOption({label:'我'});
 await expect(page.locator('.collection .grid article')).toHaveCount(1);
 await page.setViewportSize({width:1280,height:900});

 // ---- 手工搭配：卡片加入搭配并保存 ----
 await page.locator('.collection .grid article').first().getByRole('button',{name:'加入搭配'}).click();
 const composer=page.getByRole('complementary',{name:'搭配预览'});
 await expect(composer.getByText('1 件',{exact:true})).toBeVisible();
 await composer.getByLabel('搭配名称').fill('Web 端手工搭配');
 await composer.getByRole('button',{name:'保存这套穿搭'}).click();
 await expect(composer.getByRole('status')).toContainText('搭配已保存');

 // ---- 穿着记录：显式记录实际穿着 ----
 await page.getByRole('navigation').getByRole('button',{name:'记录',exact:true}).click();
 const wears=page.locator('#wears');
 await wears.getByRole('button',{name:'记录第一条穿着'}).click();
 const wearForm=page.getByRole('dialog');
 await wearForm.getByRole('button',{name:`选择 ${itemName}`}).click();
 await wearForm.getByRole('button',{name:'确认实际穿过',exact:true}).click();
 await expect(wears.getByRole('status')).toContainText('实际穿着已记录');
 await expect(wears.getByText(itemName).first()).toBeVisible();
 await expect(wears.getByText('1 次 · 1 天')).toBeVisible();

 // ---- 洞察：真实统计 ----
 await page.getByRole('button',{name:'衣橱洞察',exact:true}).click();
 await expect(page.getByRole('heading',{name:'实际穿着频率'})).toBeVisible({timeout:15000});

 // ---- 洗护记录面板可用 ----
 await page.getByRole('button',{name:'洗护记录',exact:true}).click();
 await expect(page.getByRole('button',{name:'洗护记录',exact:true})).toHaveAttribute('aria-pressed','true');
 await expect(page.getByText(/洗护/).first()).toBeVisible();

 // ---- 离线规则推荐（不依赖模型）并保存为搭配 ----
 await page.getByRole('navigation').getByRole('button',{name:'穿搭',exact:true}).click();
 await page.getByRole('button',{name:'帮我搭配',exact:true}).click();
 await expect(page.getByRole('heading',{name:'今天，想怎么穿？'})).toBeVisible();
 await page.getByRole('button',{name:'为我推荐搭配'}).click();
 await expect(page.getByText(/为你找到 \d+ 套搭配/)).toBeVisible({timeout:20000});
 await page.getByLabel('搭配保存名称').first().fill('Web 端推荐搭配');
 await page.getByRole('button',{name:'保存为搭配'}).click();
 await expect(page.getByRole('status')).toContainText('已保存到穿搭库');

 // ---- 批量录入：多图导入 → 处理 → 入库（真实任务收据） ----
 await page.getByRole('navigation').getByRole('button',{name:'衣橱',exact:true}).click();
 await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();
 const editor=page.getByRole('dialog');
 await editor.getByRole('button',{name:'批量录入',exact:true}).click();
 const batch=page.getByRole('region',{name:'批量录入'});
 await batch.locator('input[type=file]').setInputFiles([photo,photo]);
 await expect(batch.getByText('共 2 个文件')).toBeVisible();
 await expect(batch.getByText('图片已加入本地队列，点击处理待办图片后确认入库。')).toBeVisible();
 await batch.getByRole('button',{name:'处理待办图片'}).click();
 await expect(batch.getByText('待确认入库').first()).toBeVisible({timeout:30000});
 await batch.getByRole('button',{name:'入库此草稿'}).first().click();
 await expect(batch.getByText('此草稿已入库').first()).toBeVisible({timeout:15000});
 page.once('dialog',dialog=>dialog.accept());
 await editor.getByRole('button',{name:'关闭单品窗口'}).click();
 await expect(page.getByRole('dialog')).not.toBeVisible();

 // ---- 设置：能力边界如实呈现（浏览器不可用的能力明确标注） ----
 await page.getByRole('navigation').getByRole('button',{name:'设置',exact:true}).click();
 await page.getByRole('button',{name:/^模型与连接/}).click();
 await expect(page.getByRole('note')).toContainText('浏览器端不能写入系统安全存储');
 await page.getByRole('button',{name:'← 返回设置'}).click();
 await page.getByRole('button',{name:/^CLI \/ MCP 与外部 Agent/}).click();
 await expect(page.getByRole('note')).toContainText('浏览器会话不能登记、查看或撤销外部客户端授权');
 await page.getByRole('button',{name:'← 返回设置'}).click();
 await page.getByRole('button',{name:/^加密与设备/}).click();
 await expect(page.getByRole('note')).toContainText('加密空间、设备配对与安全密钥');
 await page.getByRole('button',{name:'← 返回设置'}).click();
 await page.getByRole('button',{name:/^备份与恢复/}).click();
 await expect(page.getByText('备份包含哪些资料')).toBeVisible();

 // ---- 窄屏 375px：无页面级横向溢出 ----
 await page.setViewportSize({width:375,height:800});
 await page.getByRole('navigation').getByRole('button',{name:'衣橱',exact:true}).click();
 await expect(page.getByLabel('搜索衣物',{exact:true}).first()).toBeVisible();
 const overflow=await page.evaluate(()=>document.documentElement.scrollWidth>window.innerWidth);
 expect(overflow,'375px 不应有页面级横向溢出').toBe(false);

 // ---- 键盘可达：跳到内容链接 ----
 await page.setViewportSize({width:1280,height:900});
 await page.reload();
 await page.keyboard.press('Tab');
 await expect(page.locator('.skip-link')).toBeFocused();
 await page.keyboard.press('Enter');
 await expect(page.locator('#main-content')).toBeFocused();
});
