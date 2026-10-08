import {test,expect} from '@playwright/test';

// 集中设置（Web 终验）：模型连接（真实保存，凭据能力如实标注）、穿衣偏好、
// 浏览器端备份创建与预览（不执行恢复）、问问衣橱无模型时的能力如实声明。
test.setTimeout(240_000);

const dataDir=process.env.WARDROBE_WEB_TEST_DATA!;

async function openSettings(page:import('@playwright/test').Page,card:string){
 await page.getByRole('navigation').getByRole('button',{name:'设置',exact:true}).click();
 await page.getByRole('button',{name:card}).first().click();
}

test('model connections save from the browser while keyring credentials stay desktop-only',async({page})=>{
 await page.goto('/');
 await openSettings(page,'模型与连接');
 // 能力边界如实呈现（BROWSER_LIMITS.credentials）
 await expect(page.getByRole('note')).toContainText('浏览器端不能写入系统安全存储');
 // 模型连接信息本身可以在浏览器保存（保存后对话框标题切换为“管理模型连接”）
 await page.getByRole('button',{name:'＋ 添加模型'}).click();
 const adding=page.getByRole('dialog',{name:'添加模型'});
 await adding.getByLabel('配置名称').fill('终验文本模型');
 await adding.getByLabel('兼容 API 端点').fill('https://api.example.com/v1');
 await adding.getByLabel('模型名称').fill('test-model');
 await adding.getByRole('button',{name:'保存模型配置'}).click();
 await expect(page.getByText('模型配置已保存。')).toBeVisible({timeout:30000});
 await page.getByRole('dialog').getByRole('button',{name:'关闭单品窗口'}).click();
 await expect(page.getByRole('dialog')).not.toBeVisible();
 await expect(page.locator('.provider-grid')).toContainText('终验文本模型');
 // 重载持久 + 凭据状态如实显示“未配置凭据”，且不提供浏览器写密钥入口
 await page.reload();
 await openSettings(page,'模型与连接');
 await expect(page.locator('.provider-grid')).toContainText('终验文本模型');
 await page.getByRole('button',{name:'管理连接'}).click();
 const manage=page.getByRole('dialog',{name:'管理模型连接'});
 await expect(manage).toBeVisible();
 // provider.credential.* 属于仅可信本机入口：浏览器会话连状态读取也被核心拒绝，
 // 页面如实停留在“凭据状态未读取”，且不提供任何写密钥入口（能力边界如实呈现）。
 await expect(manage.getByText(/凭据状态未读取|未配置凭据/)).toBeVisible({timeout:30000});
 await expect(manage.getByRole('note')).toContainText('浏览器端不能写入系统安全存储');
 await expect(manage.getByRole('button',{name:'保存或更换密钥'})).toHaveCount(0);
 await expect(manage.getByRole('button',{name:'移除密钥'})).toHaveCount(0);
 await manage.getByRole('button',{name:'关闭单品窗口'}).click();
});

test('explicit style preferences persist through the browser session',async({page})=>{
 await page.goto('/');
 await openSettings(page,'穿衣偏好');
 await expect(page.getByRole('heading',{name:'我声明的偏好'})).toBeVisible();
 await page.getByRole('button',{name:'＋ 添加偏好'}).click();
 const editor=page.getByRole('dialog',{name:'添加穿衣偏好'});
 await editor.getByLabel('偏好维度').selectOption({label:'颜色'});
 await editor.getByLabel('偏好值').fill('蓝色');
 await editor.getByLabel('偏好权重').fill('3');
 await editor.getByRole('button',{name:'保存明确偏好'}).click();
 await expect(page.getByText('明确偏好已保存。')).toBeVisible({timeout:30000});
 await expect(page.locator('#learning')).toContainText('颜色 · 蓝色 · 权重 3');
 await page.reload();
 await openSettings(page,'穿衣偏好');
 await expect(page.locator('#learning')).toContainText('颜色 · 蓝色 · 权重 3');
});

test('backup create and preview run through the browser session without restore',async({page})=>{
 test.info().annotations.push({type:'scope',description:'浏览器端只验收创建+预览；恢复走 ADR0022 核心验收与 web_stack 行为测试，避免在共享 e2e 数据上覆盖成员与授权'});
 await page.goto('/');
 await openSettings(page,'备份与恢复');
 const backup=page.locator('#backup-settings');
 await expect(backup.getByText('备份包含哪些资料')).toBeVisible();
 await backup.getByLabel('默认备份目录').fill(`${dataDir}/backups`);
 await backup.getByRole('button',{name:'保存备份设置'}).click();
 await expect(backup.getByText('备份设置已保存。')).toBeVisible({timeout:30000});
 // 创建备份（浏览器会话 → maintenance.worker.run → 成功收据）
 await backup.getByRole('button',{name:'创建本地备份'}).click();
 await expect(backup.getByText('备份已创建，所有列出的文件已校验。')).toBeVisible({timeout:120000});
 await expect(backup.getByLabel('维护任务')).toContainText('已完成');
 // 预览恢复范围（“恢复已有备份”面板默认折叠，先展开；不点击覆盖恢复）
 await backup.getByText('恢复已有备份').click();
 await backup.getByRole('button',{name:'预览恢复范围'}).click();
 await expect(backup.getByText('备份格式、版本和引用已验证。')).toBeVisible({timeout:120000});
 await expect(backup).toContainText('仅展示图与缩略图，不含原图');
 const row=backup.locator('table tbody tr').filter({hasText:'单品'});
 await expect(row).toBeVisible();
 const incoming=parseInt(await row.locator('td').last().innerText(),10);
 expect(incoming,'备份清单应包含本会话录入的单品').toBeGreaterThanOrEqual(4);
});

test('stylist states honestly that no text model is available',async({page})=>{
 test.info().annotations.push({type:'capability',description:'Stylist 依赖文本模型：无模型时如实反馈，不算失败'});
 await page.goto('/');
 await page.getByRole('navigation').getByRole('button',{name:'穿搭',exact:true}).click();
 await page.getByRole('button',{name:'问问衣橱'}).click();
 await expect(page.getByRole('status')).toContainText('还没有可用的文本模型。先到「设置 → 模型与连接」添加一个兼容 OpenAI 接口的文本模型。');
});

test('settings hub stays keyboard accessible and overflow-free at 375px',async({page})=>{
 await page.goto('/');
 await openSettings(page,'备份与恢复');
 await expect(page.getByText('备份包含哪些资料')).toBeVisible();
 await page.setViewportSize({width:375,height:800});
 const overflow=await page.evaluate(()=>document.documentElement.scrollWidth>window.innerWidth);
 expect(overflow,'375px 设置页不应有页面级横向溢出').toBe(false);
 // 与既有核心流程用例同型的键盘可达断言：reload 后等应用渲染完成，再 Tab 应聚焦跳到内容
 await page.setViewportSize({width:1280,height:900});
 await page.reload();
 await expect(page.getByRole('heading',{name:'我的衣橱'})).toBeVisible();
 await page.keyboard.press('Tab');
 await expect(page.locator('.skip-link')).toBeFocused();
 await page.keyboard.press('Enter');
 await expect(page.locator('#main-content')).toBeFocused();
});
