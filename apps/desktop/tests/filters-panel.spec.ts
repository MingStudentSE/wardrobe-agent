import {test,expect} from '@playwright/test';
test('filter panel applies drafts explicitly and exposes removable conditions',async({page})=>{
 await page.goto('/');const seededCount=async(page:any)=>{const r=await(await page.request.post('/__wardrobe_dev_api',{data:{operation:'clothing.list',input:{member_id:'default',status:'active',limit:100}}})).json();return r.result.items.length;};await expect.poll(async()=>await page.locator('.collection .grid article').count(),{timeout:15000}).toBeGreaterThan(0);const seeded=Math.min(await seededCount(page),24);await expect.poll(async()=>await page.locator('.collection .grid article').count(),{timeout:15000}).toBe(seeded);await page.getByRole('button',{name:/^筛选( \d+)?$/}).click();const dialog=page.getByRole('dialog',{name:'筛选衣物'});// 草稿语义：对话框内选择不即时生效，底层列表保持当前筛选结果。
await dialog.getByRole('button',{name:'回收站',exact:true}).click();await expect(page.locator('.collection .grid article')).toHaveCount(seeded);await dialog.getByRole('button',{name:'取消',exact:true}).click();await expect.poll(async()=>await page.locator('.collection .grid article').count()).toBe(seeded);
 // 回收站可能含其他规格回收的单品：以真实回收站库存断言，空则显示空态。
 const binCount=(await (await page.request.post('/__wardrobe_dev_api',{data:{operation:'clothing.list',input:{member_id:'default',status:'deleted',limit:100}}})).json()).result.items.length;
 await page.getByRole('button',{name:/^筛选( \d+)?$/}).click();await dialog.getByRole('button',{name:'回收站',exact:true}).click();await dialog.getByRole('button',{name:'应用筛选'}).click();
 if(binCount===0)await expect(page.getByText('回收站为空',{exact:true})).toBeVisible();
 else await expect(page.locator('.collection .grid article')).toHaveCount(Math.min(binCount,24));
 await page.getByRole('button',{name:'移除库存筛选'}).click();await expect.poll(async()=>await page.locator('.collection .grid article').count()).toBe(seeded);
 await page.setViewportSize({width:375,height:900});await page.getByRole('button',{name:/^筛选( \d+)?$/}).click();await dialog.locator('summary').click();expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);await page.keyboard.press('Escape');await expect(dialog).not.toBeVisible();
});
