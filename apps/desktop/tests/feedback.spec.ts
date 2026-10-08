import {test,expect,type Page} from '@playwright/test';
// 推荐反馈验收（新 UI）：穿搭 → 帮我搭配；反馈区在“推荐反馈与记录”详情内，
// 生成动作为“为我推荐搭配”。评分/历史/换套/当天排除/实际穿着确认等行为断言保持不变。
let feedbackMember='';
async function openRecommend(page:Page){
 await page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name:'穿搭',exact:true}).click();
 await page.getByRole('button',{name:'帮我搭配',exact:true}).click();
 return page.locator('#recommendations');
}
async function openFeedbackArea(panel:any){
 const summary=panel.locator('summary').filter({hasText:'推荐反馈与记录'});
 await summary.click();
 return panel.locator('.recommendation-feedback');
}
async function openStockControls(panel:any){
 await panel.locator('summary').filter({hasText:'指定想穿的单品'}).click();
}
test.beforeEach(async({page})=>{
 await page.goto('/');await page.getByText('当前衣橱',{exact:true}).click();await page.getByText('管理成员档案',{exact:true}).click();
 const name=`反馈验收-${Date.now()}`;await page.getByLabel('新成员名称').fill(name);await page.getByRole('button',{name:'创建成员',exact:true}).click();
 const choice=page.getByRole('combobox',{name:'当前成员',exact:true});await expect(choice.getByRole('option',{name,exact:true})).toBeAttached();const id=await choice.getByRole('option',{name,exact:true}).getAttribute('value');expect(id).toBeTruthy();await expect(choice).toHaveValue(id!);feedbackMember=id!;
 for(const [name,category] of [['反馈基础上衣A','top'],['反馈基础上衣B','top'],['反馈基础裤','bottom'],['反馈基础鞋','shoes']]){const response=await page.request.post('/__wardrobe_dev_api',{data:{operation:'clothing.create',input:{member_id:feedbackMember,name,category}}});expect((await response.json()).error).toBeUndefined();}
});
test.afterEach(async({page})=>{
 const choice=page.getByRole('combobox',{name:'当前成员',exact:true});
 if(!(await choice.isVisible()))await page.getByText('当前衣橱',{exact:true}).click();
 await expect(choice).toBeEnabled();await choice.selectOption({label:'我'});await expect(choice).toHaveValue(await choice.getByRole('option',{name:'我',exact:true}).getAttribute('value')??'');
});
test('推荐意见评分与历史持久，Try Another保持约束和七宽度',async({page})=>{
 await page.goto('/');const panel=await openRecommend(page);
 const call=async(operation:string,input:object={})=>{const r=await page.request.post('/__wardrobe_dev_api',{data:{operation,input:{member_id:feedbackMember,...input}}});const b=await r.json();if(b.error)throw new Error(b.error.message);return b.result;};
 await call('clothing.create',{name:'反馈裤子',category:'bottom'});await call('clothing.create',{name:'反馈鞋',category:'shoes'});
 await panel.getByRole('button',{name:'刷新推荐库存',exact:true}).click();await panel.getByRole('button',{name:'为我推荐搭配'}).click();
 const feedback=await openFeedbackArea(panel);await expect(feedback).toBeVisible();
 const before=(await call('wear.list')).items.length;await feedback.getByLabel('总体评分').selectOption('5');await feedback.getByLabel('风格评分').selectOption('4');await feedback.getByRole('button',{name:'喜欢',exact:true}).click();await expect(feedback.getByRole('status')).toContainText('反馈已保存');
 for(const width of [320,1280]){await page.setViewportSize({width,height:1000});await feedback.screenshot({path:`../../docs/verification/ticket-17/feedback-controls-${width}.png`});}await page.setViewportSize({width:1280,height:1000});
 const f=(await call('feedback.list')).items[0];expect(f.reaction).toBe('like');expect(f.scores).toEqual({overall:5,comfort:null,style:4});expect((await call('wear.list')).items.length).toBe(before);
 await feedback.getByRole('button',{name:'不喜欢',exact:true}).click();await expect(feedback.getByRole('status')).toContainText('反馈已保存');expect((await call('feedback.list')).items.some((x:any)=>x.reaction==='dislike')).toBe(true);
 await feedback.getByRole('button',{name:'跳过并换一套',exact:true}).click();await expect(panel.locator('.recommendation-proposal')).toHaveCount(1);expect((await call('feedback.list')).items.some((x:any)=>x.reaction==='skip')).toBe(true);
 await page.reload();await openRecommend(page);await openFeedbackArea(panel);const history=panel.locator('.feedback-history');await expect(history).toContainText('喜欢');await expect(history).toContainText('总体 5');
 for(const width of [320,375,414,768,1024,1280,1440]){await page.setViewportSize({width,height:1000});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBeTruthy();if(width===320||width===1280)await panel.screenshot({path:`../../docs/verification/ticket-17/feedback-${width}.png`});}
});
test('实际穿了与改穿必须UI确认，响应丢失重试只记一次穿着',async({page})=>{
 await page.goto('/');const panel=await openRecommend(page);
 const call=async(operation:string,input:object={})=>{const r=await page.request.post('/__wardrobe_dev_api',{data:{operation,input:{member_id:feedbackMember,...input}}});const b=await r.json();if(b.error)throw new Error(b.error.message);return b.result;};
 const alternative=await call('clothing.create',{name:'明确改穿上衣',category:'top'});await panel.getByRole('button',{name:'刷新推荐库存',exact:true}).click();await panel.getByRole('button',{name:'为我推荐搭配'}).click();
 const feedback=await openFeedbackArea(panel);await feedback.locator('summary').filter({hasText:'实际穿了或改穿'}).click();const before=(await call('wear.list')).items.length;
 await expect(feedback.getByRole('button',{name:'记录实际穿着反馈',exact:true})).toBeDisabled();expect((await call('wear.list')).items.length).toBe(before);
 await feedback.getByLabel('实际穿着日期',{exact:true}).fill('2020-01-01');await feedback.getByLabel('实际穿着时区',{exact:true}).fill('Pacific/Honolulu');
 await feedback.getByLabel('确认这些衣物在所填当地日期实际穿过',{exact:true}).check();let submitted=0;
 await page.route('**/__wardrobe_dev_api',async route=>{const data=route.request().postDataJSON();if(data.operation==='feedback.record'&&data.input.reaction==='actual_wore'){submitted++;const response=await route.fetch();if(submitted===1){await route.fulfill({status:503,contentType:'application/json',body:JSON.stringify({error:{message:'已记录，响应丢失'}})});return;}await route.fulfill({response});return;}await route.continue();});
 await feedback.getByRole('button',{name:'记录实际穿着反馈',exact:true}).click();await expect(panel.getByRole('alert')).toContainText('响应丢失');expect((await call('wear.list')).items.length).toBe(before+1);
 await feedback.getByRole('button',{name:'记录实际穿着反馈',exact:true}).click();await expect(feedback.getByRole('status')).toContainText('实际穿着已保存');expect((await call('wear.list')).items.length).toBe(before+1);expect(submitted).toBe(2);
 await feedback.getByLabel('实际穿着选择',{exact:true}).selectOption('changed');const selected=await feedback.locator('fieldset input[type=checkbox]:checked').count();expect(selected).toBeGreaterThan(0);
 const history=(await call('feedback.list')).items.find((f:any)=>f.reaction==='actual_wore');const oldTop=history.proposal.items.find((i:any)=>i.role==='top');await feedback.locator('fieldset').getByLabel(oldTop.name,{exact:true}).uncheck();await feedback.locator('fieldset').getByLabel('明确改穿上衣',{exact:true}).check();
 await expect(feedback.getByRole('button',{name:'记录实际穿着反馈',exact:true})).toBeDisabled();await feedback.getByLabel('确认这些衣物在所填当地日期实际穿过',{exact:true}).check();await feedback.getByRole('button',{name:'记录实际穿着反馈',exact:true}).click();await expect(feedback.getByRole('status')).toContainText('实际穿着已保存');
 const worn=(await call('wear.list')).items;expect(worn.length).toBe(before+2);const changed=(await call('feedback.list')).items.find((f:any)=>f.reaction==='changed');const event=worn.find((w:any)=>w.id===changed.wear_event_id);expect(event.local_date).toBe('2020-01-01');expect(event.timezone).toBe('Pacific/Honolulu');expect(event.items.some((i:any)=>i.clothing_id===alternative.id)).toBe(true);expect(event.items.some((i:any)=>i.clothing_id===oldTop.clothing_id)).toBe(false);
 for(const width of [320,1280]){await page.setViewportSize({width,height:1000});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBeTruthy();await feedback.screenshot({path:`../../docs/verification/ticket-17/feedback-actual-${width}.png`});}
});
test('UI当天排除与固定冲突、反馈更正撤销历史均不改变穿着',async({page})=>{
 await page.goto('/');const panel=await openRecommend(page);
 const call=async(operation:string,input:object={})=>{const r=await page.request.post('/__wardrobe_dev_api',{data:{operation,input:{member_id:feedbackMember,...input}}});const b=await r.json();if(b.error)throw new Error(b.error.message);return b.result;};
 await call('clothing.create',{name:'当天固定上衣',category:'top'});await panel.getByRole('button',{name:'刷新推荐库存',exact:true}).click();await openStockControls(panel);await panel.getByLabel('搜索推荐库存',{exact:true}).fill('当天固定上衣');await panel.getByLabel('固定 当天固定上衣',{exact:true}).check();await panel.getByRole('button',{name:'为我推荐搭配'}).click();await openFeedbackArea(panel);const feedback=panel.locator('.recommendation-feedback');const before=(await call('wear.list')).items.length;
 await feedback.locator('summary').filter({hasText:'当天排除所选单品'}).click();const day=feedback.locator('details').filter({has:page.getByRole('button',{name:'确认当天排除',exact:true})});for(const checkbox of await day.getByRole('checkbox').all())await checkbox.uncheck();
 await day.getByLabel('当天固定上衣',{exact:true}).check();await day.getByRole('button',{name:'确认当天排除',exact:true}).click();await expect(feedback.getByRole('status')).toContainText('反馈已保存');
 await panel.getByRole('button',{name:'为我推荐搭配'}).click();await expect(panel.getByRole('alert')).toContainText('当天排除');expect((await call('wear.list')).items.length).toBe(before);
 const history=panel.locator('.feedback-history');await history.getByRole('button',{name:/查看或更正反馈/}).first().click();await history.getByLabel('更正舒适评分',{exact:true}).selectOption('2');await history.getByRole('button',{name:'保存反馈更正',exact:true}).click();await expect(history).toContainText('舒适 2');await history.getByLabel('撤销反馈原因',{exact:true}).fill('不再排除');const revokedResponse=page.waitForResponse(r=>r.url().includes('__wardrobe_dev_api')&&r.request().postDataJSON().operation==='feedback.revoke');await history.getByRole('button',{name:'撤销此反馈',exact:true}).click();const revoked=(await (await revokedResponse).json()).result;expect(revoked.revoked).toBe(true);
 const reappliedResponse=page.waitForResponse(r=>r.url().includes('__wardrobe_dev_api')&&r.request().postDataJSON().operation==='feedback.record'&&r.request().postDataJSON().input.reaction==='day_exclude');await day.getByRole('button',{name:'确认当天排除',exact:true}).click();const reapplied=(await (await reappliedResponse).json()).result;expect(reapplied.root_id).not.toBe(revoked.root_id);expect((await call('feedback.list')).items.some((f:any)=>f.reaction==='day_exclude'&&!f.revoked)).toBe(true);
 await history.getByRole('button',{name:/查看或更正反馈/}).first().click();await history.getByLabel('撤销反馈原因',{exact:true}).fill('再次撤销');await history.getByRole('button',{name:'撤销此反馈',exact:true}).click();
 await history.getByLabel('包含已撤销反馈',{exact:true}).check();await expect(history).toContainText('已撤销');expect((await call('wear.list')).items.length).toBe(before);
 await panel.getByRole('button',{name:'为我推荐搭配'}).click();await expect(panel.locator('.recommendation-proposal').first()).toContainText('当天固定上衣');
});
