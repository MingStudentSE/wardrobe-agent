import {test,expect} from '@playwright/test';
test('details edit, archive, recycle and restore clothing without losing facts',async({page})=>{
 await page.goto('/');await expect(page.getByRole('article')).toHaveCount(24);
 await page.getByRole('article').first().getByRole('button',{name:'查看详情'}).click();
 await page.getByLabel('名称',{exact:true}).fill('回收站验收单品');await page.getByText('属性与购买信息',{exact:true}).click();await page.getByLabel('材质',{exact:true}).fill('棉，麻');await page.getByLabel('品牌',{exact:true}).fill('手工品牌');await page.getByRole('button',{name:'保存修改'}).click();await expect(page.getByRole('status')).toContainText('已更新');
 await page.getByRole('button',{name:'归档单品',exact:true}).click();await page.getByLabel('生命周期').selectOption('archived');
 await page.getByRole('article').filter({hasText:'回收站验收单品'}).getByRole('button',{name:'查看详情'}).click();await page.getByText('属性与购买信息',{exact:true}).click();await expect(page.getByLabel('品牌',{exact:true})).toHaveValue('手工品牌');
 await page.getByRole('button',{name:'移入回收站',exact:true}).click();await page.getByRole('button',{name:'确认移入'}).click();await page.getByLabel('生命周期').selectOption('deleted');
 await page.getByRole('article').filter({hasText:'回收站验收单品'}).getByRole('button',{name:'查看详情'}).click();await page.getByRole('button',{name:'恢复单品'}).click();await page.getByLabel('生命周期').selectOption('archived');
 await expect(page.getByRole('article').filter({hasText:'回收站验收单品'})).toHaveCount(1);
 await page.getByRole('article').filter({hasText:'回收站验收单品'}).getByRole('button',{name:'查看详情'}).click();await page.getByRole('button',{name:'取消归档'}).click();
});

test('management controls remain reachable at supported narrow and desktop widths',async({page})=>{
 for(const width of [320,375,414,768,1024,1280,1440]) {await page.setViewportSize({width,height:900});await page.goto('/');await expect(page.getByRole('article').first()).toBeVisible();expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBeTruthy();await page.getByRole('article').first().getByRole('button',{name:'查看详情'}).click();await expect(page.getByRole('button',{name:'移入回收站',exact:true})).toBeVisible();await page.screenshot({path:`/tmp/wardrobe-ticket-04-ui-${width}.png`,fullPage:true});}
});
