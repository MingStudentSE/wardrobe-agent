import {test,expect,type Page} from '@playwright/test';
// 四工作区 UI 后的推荐验收：穿搭 → 帮我搭配；推荐动作统一为“为我推荐搭配”，
// 固定/排除/优先在“指定想穿的单品”详情内；行为断言（硬约束、替换、换套、保存、重启、幂等）保持不变。
const nav=(page:Page,name:string)=>page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name,exact:true});
async function openRecommend(page:Page){
 await nav(page,'穿搭').click();
 await page.getByRole('button',{name:'帮我搭配',exact:true}).click();
 return page.locator('#recommendations');
}
async function openStockControls(panel:any){
 await panel.locator('summary').filter({hasText:'指定想穿的单品'}).click();
}
test('离线推荐硬约束、替换换套、显式保存与重启不记穿着',async({page})=>{
 await page.goto('/');
 const panel=await openRecommend(page);
 const call=async(operation:string,input:object={})=>{const response=await page.request.post('/__wardrobe_dev_api',{data:{operation,input:{member_id:'default',...input}}});const body=await response.json();if(body.error)throw new Error(body.error.message);return body.result;};
 const bottom=await call('clothing.create',{name:'推荐裤子',category:'bottom'});await call('clothing.create',{name:'推荐鞋',category:'shoes'});await panel.getByRole('button',{name:'刷新推荐库存',exact:true}).click();await openStockControls(panel);await panel.getByLabel('固定 推荐裤子').check();await panel.getByLabel('排除 分页单品 25').check();
 await panel.getByRole('button',{name:'为我推荐搭配'}).click();await expect(panel.locator('.recommendation-proposal')).toHaveCount(3);const first=panel.locator('.recommendation-proposal').first();await expect(first).toContainText('推荐裤子');await expect(first).not.toContainText('分页单品 25');
 const before=await call('wear.list');await first.locator('li').filter({hasText:'上装'}).getByRole('button',{name:/^替换 /}).click();await expect(panel.locator('.recommendation-proposal')).toHaveCount(1);await panel.getByRole('button',{name:'换一套',exact:true}).click();await expect(panel.locator('.recommendation-proposal')).toHaveCount(1);
 await panel.getByLabel('搭配保存名称').fill('离线推荐保存');await panel.getByRole('button',{name:'保存为搭配',exact:true}).click();await expect(panel.getByRole('status')).toContainText('已保存到穿搭库');expect((await call('wear.list')).items).toEqual(before.items);expect((await call('outfit.list')).items.some((o:any)=>o.name==='离线推荐保存'&&o.items.some((i:any)=>i.clothing_id===bottom.id))).toBe(true);
 await page.reload();await openRecommend(page);await panel.getByRole('button',{name:/查看推荐/}).first().click();await expect(panel.locator('.recommendation-proposal')).toHaveCount(1);
 for(const width of[320,375,414,768,1024,1280,1440]){await page.setViewportSize({width,height:1000});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBeTruthy();if(width===320||width===1280)await panel.screenshot({path:`../../docs/verification/ticket-15/recommendations-${width}.png`});}
});
test('推荐已提交响应丢失重试保持同一意图与成员锁',async({page})=>{
 await page.goto('/');const panel=await openRecommend(page);await expect(panel.getByRole('button',{name:'为我推荐搭配'})).toBeEnabled();const before=(await(await page.request.post('/__wardrobe_dev_api',{data:{operation:'recommendation.list',input:{member_id:'default'}}})).json()).result.items.length;let calls=0;
 await page.route('**/__wardrobe_dev_api',async route=>{if(route.request().postDataJSON().operation==='recommendation.generate'){calls++;const response=await route.fetch();if(calls===1){await new Promise(r=>setTimeout(r,300));await route.fulfill({status:503,contentType:'application/json',body:JSON.stringify({error:{message:'推荐已提交，响应丢失'}})});return;}await route.fulfill({response});return;}await route.continue();});
 await panel.getByRole('button',{name:'为我推荐搭配'}).click();await expect(page.getByRole('combobox',{name:'当前成员',exact:true})).toBeDisabled();await expect(panel.getByRole('alert')).toContainText('响应丢失');await panel.getByRole('button',{name:'为我推荐搭配'}).click();await expect(panel.locator('.recommendation-proposal')).toHaveCount(3);const after=(await(await page.request.post('/__wardrobe_dev_api',{data:{operation:'recommendation.list',input:{member_id:'default'}}})).json()).result.items.length;expect(after).toBe(before+1);expect(calls).toBe(2);
});
test('迟到推荐读取不会跨成员显示',async({page})=>{
 await page.goto('/');const panel=await openRecommend(page);await expect(panel.getByRole('button',{name:/查看推荐/}).first()).toBeEnabled();let release!:()=>void;const gate=new Promise<void>(r=>release=r);let arrived=false;
 await page.route('**/__wardrobe_dev_api',async route=>{if(route.request().postDataJSON().operation==='recommendation.get'){const response=await route.fetch();arrived=true;await gate;await route.fulfill({response});return;}await route.continue();});await panel.getByRole('button',{name:/查看推荐/}).first().click();await expect.poll(()=>arrived).toBe(true);
 await page.getByText('当前衣橱',{exact:true}).click();await page.getByText('管理成员档案',{exact:true}).click();await page.getByLabel('新成员名称').fill('推荐隔离成员');await page.getByRole('button',{name:'创建成员',exact:true}).click();await expect(page.locator('#recommendations').locator('summary').filter({hasText:'推荐反馈与记录'})).toBeVisible();const panel2=page.locator('#recommendations');await panel2.locator('summary').filter({hasText:'推荐反馈与记录'}).click();await expect(panel2).toContainText('还没有推荐记录');release();await page.waitForTimeout(100);await expect(panel2.locator('.recommendation-proposal')).toHaveCount(0);await expect(panel2.getByRole('alert')).toHaveCount(0);
 const choice=page.getByRole('combobox',{name:'当前成员',exact:true});
 await expect(choice).toBeEnabled();await choice.selectOption({label:'我'});
});
