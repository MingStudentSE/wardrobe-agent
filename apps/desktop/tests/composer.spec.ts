import {test,expect} from '@playwright/test';
test('cards keep selected garments across filters and save a real outfit',async({page})=>{
 await page.goto('/');const cards=page.locator('.collection .grid article');const seededCount=async(page:any)=>{const r=await(await page.request.post('/__wardrobe_dev_api',{data:{operation:'clothing.list',input:{member_id:'default',status:'active',limit:100}}})).json();return r.result.items.length;};await expect.poll(async()=>await cards.count(),{timeout:15000}).toBeGreaterThan(0);const seeded=Math.min(await seededCount(page),24);await expect.poll(async()=>await cards.count(),{timeout:15000}).toBe(seeded);
 await cards.nth(0).getByRole('button',{name:'加入搭配'}).click();await cards.nth(1).getByRole('button',{name:'加入搭配'}).click();
 const composer=page.getByRole('complementary',{name:'搭配预览'});await expect(composer.getByText('2 件',{exact:true})).toBeVisible();
 await page.getByRole('button',{name:'外套',exact:true}).click();await expect(composer.getByText('2 件',{exact:true})).toBeVisible();
 await composer.getByRole('button',{name:/移除/}).first().click();await expect(composer.getByText('1 件',{exact:true})).toBeVisible();
 const outfitsBefore=(await (await page.request.post('/__wardrobe_dev_api',{data:{operation:'outfit.list',input:{member_id:'default',status:'active'}}})).json()).result.items.length;
 await composer.getByLabel('搭配名称').fill('卡片组合验收');await composer.getByRole('button',{name:'保存这套穿搭'}).click();await expect(composer.getByRole('status')).toContainText('搭配已保存');
 await page.reload();await expect.poll(async()=>await page.getByText(/^\d+ 件单品 · \d+ 套穿搭$/).count(),{timeout:15000}).toBeGreaterThan(0);
 // 共享 dev server 上其他规格可能已保存穿搭：断言保存后计数恰好 +1。
 await expect(page.getByText(/^\d+ 件单品 · \d+ 套穿搭$/)).toContainText(`${outfitsBefore+1} 套穿搭`);
 for(const width of [320,768,1440]){await page.setViewportSize({width,height:900});await expect.poll(()=>page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);}
});
