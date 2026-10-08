import {test,expect} from '@playwright/test';
import {restoreDefaultMember} from './support/member-hygiene';
// 洗护记录验收（新 UI）：记录 → 洗护记录；记录/更正表单在弹窗中，时区在“时间与时区”详情内。
// 行为断言（阈值、手动待洗、干洗、更正撤销、修订历史、幂等重试、成员锁与隔离）保持不变。
const nav=(page:any,name:string)=>page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name,exact:true});
const panel=(page:any)=>page.locator('#care');
async function openCare(page:any){
 await nav(page,'记录').click();
 await page.getByRole('button',{name:'洗护记录',exact:true}).click();
 return panel(page);
}
test('真实Application洗护阈值、手动待洗、干洗、更正撤销与重启',async({page})=>{
 await page.goto('/');
 const p=await openCare(page);
 await p.getByRole('button',{name:'＋ 记录洗护',exact:true}).click();
 let d=page.getByRole('dialog',{name:'记录洗护',exact:true});
 await d.getByLabel('选择洗护单品').selectOption({label:'分页单品 17'});
 await expect(d).toContainText('最近有效清洗后 0 次');
 await d.getByLabel('洗护阈值',{exact:true}).fill('2');
 await d.getByRole('button',{name:'保存阈值',exact:true}).click();
 // 保存反馈显示在面板级状态区（弹窗模态之下仍渲染）。
 await expect(p.getByRole('status')).toContainText('阈值已保存');
 await d.getByLabel('洗护类型').selectOption('mark_dirty');
 await d.getByLabel('洗护日期').fill('2020-01-01');
 await d.locator('details').filter({hasText:'时间与时区'}).locator('summary').click();
 await d.getByLabel('洗护时区').fill('UTC');
 await d.getByRole('button',{name:'确认记录洗护',exact:true}).click();
 await expect(p.getByRole('status')).toContainText('洗护已保存');
 await expect(p).toContainText('自动推荐排除');
 await expect(p.getByRole('heading',{name:'待洗列表'}).locator('..')).toContainText('分页单品 17');
 // 干洗后解除待洗并进入可推荐。
 await p.getByRole('button',{name:'＋ 记录洗护',exact:true}).click();
 d=page.getByRole('dialog',{name:'记录洗护',exact:true});
 await d.getByLabel('选择洗护单品').selectOption({label:'分页单品 17'});
 await d.getByLabel('洗护类型').selectOption('dry_clean');
 await d.getByLabel('洗护日期').fill('2020-01-02');
 await d.getByRole('button',{name:'确认记录洗护',exact:true}).click();
 await expect(p).toContainText('没有待洗单品');
 // 提交后弹窗关闭：重新打开并选中该单品验证回到“可推荐”。
 await p.getByRole('button',{name:'＋ 记录洗护',exact:true}).click();
 d=page.getByRole('dialog',{name:'记录洗护',exact:true});
 await d.getByLabel('选择洗护单品').selectOption({label:'分页单品 17'});
 await expect(d.getByText('可推荐')).toBeVisible();
 await d.getByRole('button',{name:'关闭单品窗口'}).click();
 // 更正干洗日期更早，仍可推荐；随后撤销并检查修订历史。
 const clean=p.locator('.care-events li').filter({hasText:'分页单品 17 · 干洗'}).first();
 await clean.getByRole('button',{name:'更正洗护',exact:true}).click();
 d=page.getByRole('dialog',{name:'更正洗护记录',exact:true});
 await d.getByLabel('洗护日期').fill('2019-12-31');
 await d.getByRole('button',{name:'确认更正洗护',exact:true}).click();
 await expect(p.getByRole('status')).toContainText('洗护已保存');
 // 更正后的有效时间线：干洗（2019-12-31）早于手动标记（2020-01-01），标记仍然有效。
 await p.getByRole('button',{name:'＋ 记录洗护',exact:true}).click();
 d=page.getByRole('dialog',{name:'记录洗护',exact:true});
 await d.getByLabel('选择洗护单品').selectOption({label:'分页单品 17'});
 await expect(d.getByText('自动推荐排除')).toBeVisible();
 await expect(d.getByText(/自动推荐排除 · 手动待洗/)).toBeVisible();
 await d.getByRole('button',{name:'关闭单品窗口'}).click();
 const revoked=p.locator('.care-events li').filter({hasText:'分页单品 17 · 干洗'}).first();
 await revoked.getByRole('button',{name:'撤销洗护',exact:true}).click();
 const rev=page.getByRole('dialog',{name:'撤销洗护记录',exact:true});
 await rev.getByLabel('洗护撤销原因').fill('误记');
 await rev.getByRole('button',{name:'确认撤销洗护',exact:true}).click();
 await expect(p.getByRole('status')).toContainText('已撤销');
 await p.locator('.care-events li').filter({hasText:'已撤销'}).first().getByRole('button',{name:'查看洗护修订'}).click();
 await expect(page.getByRole('dialog',{name:'洗护修改历史',exact:true})).toContainText('修订 3');
 await page.getByRole('dialog').getByRole('button',{name:'关闭单品窗口'}).click();
 await page.reload();
 await openCare(page);
 await expect(p).toContainText('手动标记');
 for(const width of[320,375,414,768,1024,1280,1440]){await page.setViewportSize({width,height:1000});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBeTruthy();if(width===320||width===1280)await p.screenshot({path:`../../docs/verification/ticket-10/care-${width}.png`});}
});
test('已提交响应丢失重试不重复洗护且成员锁保持',async({page})=>{
 await page.goto('/');
 const p=await openCare(page);
 await p.getByRole('button',{name:'＋ 记录洗护',exact:true}).click();
 const d=page.getByRole('dialog',{name:'记录洗护',exact:true});
 await d.getByLabel('选择洗护单品').selectOption({label:'分页单品 16'});
 await d.getByLabel('洗护日期').fill('2020-01-02');
 let calls=0;await page.route('**/__wardrobe_dev_api',async route=>{if(route.request().postDataJSON().operation==='care.record'){calls++;const response=await route.fetch();if(calls===1){await new Promise(r=>setTimeout(r,300));await route.fulfill({status:503,contentType:'application/json',body:JSON.stringify({error:{message:'已提交响应丢失'}})});return;}await route.fulfill({response});return;}await route.continue();});
 await d.getByRole('button',{name:'确认记录洗护',exact:true}).click();
 await expect(page.locator('.member-switcher select')).toBeDisabled();
 await expect(d.getByRole('alert')).toContainText('响应丢失');
 await d.getByRole('button',{name:'确认记录洗护',exact:true}).click();
 await expect(p.getByRole('status')).toContainText('洗护已保存');
 expect(await p.locator('.care-events li').filter({hasText:'分页单品 16'}).count()).toBe(1);
 expect(calls).toBe(2);
});
test('迟到的真实洗护修订读取不会显示到新成员',async({page})=>{
 await page.goto('/');
 const p=await openCare(page);
 await expect(p.locator('.care-events li').filter({hasText:'分页单品 16'})).toHaveCount(1);
 let release!:()=>void;const gate=new Promise<void>(r=>release=r);let arrived=false;
 await page.route('**/__wardrobe_dev_api',async route=>{if(route.request().postDataJSON().operation==='care.history'){const response=await route.fetch();arrived=true;await gate;await route.fulfill({response});return;}await route.continue();});
 await p.locator('.care-events li').filter({hasText:'分页单品 16'}).getByRole('button',{name:'查看洗护修订'}).click();
 await expect.poll(()=>arrived).toBe(true);
 // 响应被扣住时修订历史弹窗尚未渲染；直接经侧栏切换成员。
 await page.getByText('当前衣橱',{exact:true}).click();
 await page.getByText('管理成员档案',{exact:true}).click();
 await page.getByLabel('新成员名称').fill('洗护隔离成员');
 await page.getByRole('button',{name:'创建成员',exact:true}).click();
 // 切换成员后应用回到衣橱且洗护面板卸载：迟到响应不得写入任何成员视图。
 release();
 await page.waitForTimeout(150);
 await nav(page,'记录').click();
 await page.getByRole('button',{name:'洗护记录',exact:true}).click();
 const fresh=page.locator('#care');
 await expect(fresh).toBeVisible();
 await expect(fresh.getByRole('dialog',{name:'洗护修改历史',exact:true})).toHaveCount(0);
 await expect(fresh).not.toContainText('分页单品 16');
 await expect(fresh.getByRole('alert')).toHaveCount(0);
 const choice=page.getByRole('combobox',{name:'当前成员',exact:true});
 await expect(choice).toBeEnabled();
 await choice.selectOption({label:'我'});
});
test.afterEach(async({request})=>{await restoreDefaultMember(request);});
