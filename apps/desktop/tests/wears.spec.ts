import {test,expect} from '@playwright/test';
// 四工作区 UI（#38）后的穿着记录验收：到达路径为 记录 → 穿着记录，
// 记录表单在弹窗中完成；行为断言（补记、同日换装、改穿、撤销与事件历史）保持不变。
const nav=(page:any,name:string)=>page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name,exact:true});
const wearPanel=(page:any)=>page.locator('#wears');
async function openWearDialog(page:any){
 await nav(page,'记录').click();
 await page.getByRole('button',{name:'穿着记录',exact:true}).click();
 const p=wearPanel(page);
 await p.getByRole('button',{name:'＋ 记录穿着',exact:true}).click();
 return page.getByRole('dialog',{name:'记录穿着',exact:true});
}
async function pickItem(page:any,dialog:any,name:string){
 await dialog.getByRole('button',{name:`选择 ${name}`,exact:true}).click();
}
test('穿着补记、同日换装、改穿及撤销保留事件历史',async({page})=>{
 await page.goto('/');
 const p=wearPanel(page);
 const d=await openWearDialog(page);
 await d.getByLabel('穿着日期',{exact:true}).fill('2020-01-02');
 await d.locator('details').filter({hasText:'时间与时区'}).locator('summary').click();
 await d.getByLabel('穿着时区',{exact:true}).fill('Asia/Shanghai');
 await pickItem(page,d,'分页单品 22');
 await d.getByRole('button',{name:'确认实际穿过',exact:true}).click();
 await expect(p.getByRole('status')).toContainText('实际穿着已记录');
 await expect(p.locator('.journal-entry').filter({hasText:'分页单品 22'})).toHaveCount(1);
 // 同日再穿：最常穿指标显示 2 次 / 1 天。
 await p.getByRole('button',{name:'＋ 记录穿着',exact:true}).click();
 const d2=page.getByRole('dialog',{name:'记录穿着',exact:true});
 await d2.getByLabel('穿着日期',{exact:true}).fill('2020-01-02');
 await pickItem(page,d2,'分页单品 22');
 await d2.getByRole('button',{name:'确认实际穿过',exact:true}).click();
 await expect(p.getByRole('status')).toContainText('实际穿着已记录');
 await expect(p.locator('.journal-metrics article').filter({hasText:'最常穿的单品'})).toContainText('2 次 · 1 天');
 // 改穿：移出 22，换成 21，旧事件保留且新事件生效。
 await p.locator('.journal-entry').filter({hasText:'分页单品 22'}).first().getByRole('button',{name:'更正记录'}).click();
 const d3=page.getByRole('dialog',{name:'更正穿着记录',exact:true});
 await d3.getByRole('button',{name:'移出实际穿着 分页单品 22',exact:true}).click();
 await pickItem(page,d3,'分页单品 21');
 await d3.getByRole('button',{name:'确认更正',exact:true}).click();
 await expect(p.getByRole('status')).toContainText('实际穿着已更正');
 await expect(p.locator('.journal-entry').filter({hasText:'分页单品 21'})).toHaveCount(1);
 // 撤销：原因必填，历史中保留撤销事件。
 await p.locator('.journal-entry').filter({hasText:'分页单品 21'}).getByRole('button',{name:'撤销记录'}).click();
 const d4=page.getByRole('dialog',{name:'撤销穿着记录',exact:true});
 await d4.getByLabel('撤销原因').fill('误记');
 await d4.getByRole('button',{name:'确认撤销',exact:true}).click();
 await expect(p.getByRole('status')).toContainText('实际穿着已撤销');
 await p.getByLabel('显示已撤销').check();
 await expect(p.locator('.journal-entry').filter({hasText:'分页单品 21'}).filter({hasText:'已撤销'})).toHaveCount(1);
 await p.locator('.journal-entry').filter({hasText:'分页单品 21'}).getByRole('button',{name:'修改历史',exact:true}).click();
 await expect(page.getByRole('dialog',{name:'穿着修改历史',exact:true})).toContainText('撤销');
 await page.getByRole('dialog').getByRole('button',{name:'关闭单品窗口'}).click();
 await page.reload();
 await nav(page,'记录').click();
 await page.getByRole('button',{name:'穿着记录',exact:true}).click();
 // 撤销跨重启保留：勾选“显示已撤销”后时间线仍含该事件。
 await p.getByLabel('显示已撤销').check();
 await expect(p.locator('.journal-entry').filter({hasText:'分页单品 21'}).filter({hasText:'已撤销'})).toHaveCount(1);
});
test('模板今天穿与单品穿过均需显式确认，保存模板不产生穿着',async({page})=>{
 await page.goto('/');
 // 保存模板：衣橱加入搭配 → 搭配预览保存，不产生穿着。
 await page.locator('.collection .grid article').first().getByRole('button',{name:'加入搭配'}).click();
 const composer=page.getByRole('complementary',{name:'搭配预览'});
 await composer.getByLabel('搭配名称').fill('今日模板');
 await composer.getByRole('button',{name:'保存这套穿搭'}).click();
 await expect(composer.getByRole('status')).toContainText('搭配已保存');
 const call=async(operation:string,input:object)=>{const r=await(await page.request.post('/__wardrobe_dev_api',{data:{operation,input}})).json();if(r.error)throw new Error(r.error.message);return r.result;};
 const defaultWears=(await call('wear.list',{member_id:'default'})).items.length;
 // 模板今天穿：穿搭库 → 弹窗内显式确认，跳到穿着记录表单。
 await nav(page,'穿搭').click();
 await page.getByRole('button',{name:'编辑 今日模板',exact:true}).click();
 const dialog=page.getByRole('dialog',{name:'调整穿搭',exact:true});
 await dialog.getByRole('button',{name:'模板今天穿',exact:true}).click();
 const wearDialog=page.getByRole('dialog',{name:'记录穿着',exact:true});
 await expect(wearDialog).toBeVisible();
 await wearDialog.getByRole('button',{name:'确认实际穿过',exact:true}).click();
 // #wears 内图片加载等也会渲染 role=status：按成功文案过滤后断言。
 await expect(page.locator('#wears').getByRole('status').filter({hasText:'实际穿着已记录'})).toBeVisible();
 // 模板来源穿着确认后，主界面会自行关闭“调整穿搭”弹窗。
 await expect(page.getByRole('dialog',{name:'调整穿搭',exact:true})).toHaveCount(0);
 expect((await call('wear.list',{member_id:'default'})).items.length).toBe(defaultWears+1);
 // 单品“今天穿了”：同样需要显式确认。
 await nav(page,'衣橱').click();
 await page.locator('.collection .grid article').filter({hasText:'分页单品 19'}).getByRole('button',{name:'今天穿了',exact:true}).click();
 const single=page.getByRole('dialog',{name:'记录穿着',exact:true});
 await expect(single).toBeVisible();
 await single.getByRole('button',{name:'确认实际穿过',exact:true}).click();
 await expect(page.locator('#wears').getByRole('status').filter({hasText:'实际穿着已记录'})).toBeVisible();
 await expect(page.locator('#wears .journal-entry').filter({hasText:'分页单品 19'})).toHaveCount(1);
 for(const width of [320,375,414,768,1024,1280,1440]){await page.setViewportSize({width,height:1000});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBeTruthy();if(width===320||width===1280)await page.locator('#wears').screenshot({path:`../../docs/verification/ticket-09/wears-${width}.png`});}
});
test('提交中阻止换成员，失败重试复用已提交幂等键',async({page})=>{
 await page.goto('/');
 const p=wearPanel(page);
 const d=await openWearDialog(page);
 await d.getByLabel('穿着日期',{exact:true}).fill('2020-01-04');
 await pickItem(page,d,'分页单品 18');
 let requests=0;await page.route('**/__wardrobe_dev_api',async route=>{if(route.request().postDataJSON().operation==='wear.record'){requests++;const response=await route.fetch();if(requests===1){await new Promise(r=>setTimeout(r,500));await route.fulfill({status:503,contentType:'application/json',body:JSON.stringify({error:{message:'已提交但响应丢失，请重试'}})});return;}await route.fulfill({response});return;}await route.continue();});
 await d.getByRole('button',{name:'确认实际穿过',exact:true}).click();
 // 当前衣橱详情未展开：成员锁用 CSS 定位断言（attached + disabled）。
 await expect(page.locator('.member-switcher select')).toBeDisabled();
 await expect(d.getByRole('alert')).toContainText('响应丢失');
 await d.getByRole('button',{name:'确认实际穿过',exact:true}).click();
 await expect(p.locator('.journal-entry').filter({hasText:'分页单品 18'})).toHaveCount(1);
 expect(requests).toBe(2);
 // 未来日期在提交时被拒绝，草稿保留。
 await p.getByRole('button',{name:'＋ 记录穿着',exact:true}).click();
 const d2=page.getByRole('dialog',{name:'记录穿着',exact:true});
 await d2.getByLabel('穿着日期',{exact:true}).fill('2999-01-01');
 await pickItem(page,d2,'分页单品 18');
 await d2.getByRole('button',{name:'确认实际穿过',exact:true}).click();
 await expect(d2.getByRole('alert')).toContainText('未来日期');
});
