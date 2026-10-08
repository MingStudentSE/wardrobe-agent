import {test,expect} from '@playwright/test';
import {restoreDefaultMember} from './support/member-hygiene';
// Ticket-23 GUI acceptance (issue #23): the 场景计划 tab drives the real
// Application API through the development bridge (no mock):
//  - 婚礼/旅行计划基于真实库存生成，逐天方案与去重打包清单可见；
//  - 固定/排除/手工说明可用；缺库存给短缺原因；
//  - 计划可查看、修改（乐观锁重算）与保存；保存的穿搭模板不记穿着；
//  - 键盘可达（原生 input/button，无 hover 依赖），320px 无溢出。
// 本 spec 只走本地规则路径（确定性、无网络）；模型通道的协议与降级语义
// 由 wardrobe-core tests/scenarios.rs 的本地脚本化 HTTP oracle 覆盖
// （真实模型成功需用户自配密钥，此处如实不覆盖）。

let page:import('@playwright/test').Page|null=null;
async function api(operation:string,input:object):Promise<{result?:any;error?:any}>{
 const response=await page!.request.post('/__wardrobe_dev_api',{data:{operation,input}});
 return response.json() as Promise<{result?:any;error?:any}>;
}
async function openScenarios(browserPage:import('@playwright/test').Page){
 await browserPage.goto('/');
 await browserPage.getByRole('button',{name:'穿搭',exact:true}).click();
 await browserPage.getByRole('button',{name:'场景计划',exact:true}).click();
 const panel=browserPage.locator('#scenarios');
 await expect(panel).toBeVisible();
 return panel;
}

test.beforeEach(async({request})=>{await restoreDefaultMember(request);});
test('scenario tab plans a wedding and a deduped travel checklist without recording wears',async({page:browserPage})=>{
 page=browserPage;
 const panel=await openScenarios(browserPage);
 const space=await api('space.default',{});
 const member=space.result.member_id as string;
 // 真实库存：礼服与一套正式上下装、共用皮鞋；备用上下装用于跨天轮换。
 const dress=await api('clothing.create',{member_id:member,name:'深蓝礼服连衣裙',category:'dress',occasion:['formal']});
 const suitTop=await api('clothing.create',{member_id:member,name:'白衬衫',category:'top',occasion:['formal']});
 const suitBottom=await api('clothing.create',{member_id:member,name:'西裤',category:'bottom',occasion:['formal']});
 const shoes=await api('clothing.create',{member_id:member,name:'黑色皮鞋',category:'shoes',occasion:['formal']});
 const altTop=await api('clothing.create',{member_id:member,name:'备用衬衫',category:'top'});
 const altBottom=await api('clothing.create',{member_id:member,name:'备用长裤',category:'bottom'});
 expect(dress.result.id).toBeTruthy();
 // 共享 dev server 上默认成员已有历史穿着：断言“计划与模板不新增穿着”用增量。
 const wearsBaseline=(await api('wear.list',{member_id:member})).result.items.length;
 // 面板挂载早于入库：显式刷新后再操作库存清单。
 await panel.getByRole('button',{name:'刷新计划列表'}).click();
 await panel.locator('.scenario-items summary').click();
 await expect(panel.getByRole('checkbox',{name:`固定 ${dress.result.name}`})).toBeVisible({timeout:20000});

 // 婚礼：固定连衣裙并排除当前库存中全部鞋（确定性短缺：无鞋则方案必然不完整）。
 // 其他规格可能向共享库存添加鞋，逐双排除以保持“无鞋可用”的前提。
 await panel.getByLabel('标题（可选）').fill('表姐的婚礼');
 await panel.getByLabel('手工说明（可选）').fill('婚礼在室内，按室温准备');
 await panel.getByRole('checkbox',{name:`固定 ${dress.result.name}`}).check();
 const stockNow=(await api('clothing.list',{member_id:member,status:'active',limit:100})).result.items as {name:string;category:string}[];
 for(const shoe of stockNow.filter((s:{category:string})=>s.category==='shoes'))await panel.getByRole('checkbox',{name:`排除 ${shoe.name}`}).check();
 await panel.getByRole('button',{name:'生成计划'}).click();
 await expect(panel.getByRole('status').filter({hasText:'计划已生成并保存'})).toBeVisible({timeout:20000});
 await expect(panel.getByText('第 1 天').first()).toBeVisible();
 await expect(panel.locator('.scenario-day').first()).toContainText('深蓝礼服连衣裙');
 await expect(panel.locator('.scenario-shortage')).toContainText('缺少');
 await expect(panel.locator('.scenario-note').first()).toContainText('不会记成实际穿着');
 // 手工说明随婚礼计划保存。
 await expect(panel.locator('.scenario-result')).toContainText('婚礼在室内，按室温准备');

 // 旅行：排除备用衬衫；三天轮换 + 打包清单去重。
 await panel.getByRole('button',{name:'多日旅行',exact:true}).click();
 await panel.getByLabel('旅行天数').fill('3');
 await panel.getByLabel('标题（可选）').fill('三日出行');
 await panel.getByRole('checkbox',{name:`固定 ${dress.result.name}`}).uncheck();
 await panel.getByRole('checkbox',{name:`排除 ${shoes.result.name}`}).uncheck();
 await panel.getByRole('checkbox',{name:`排除 ${altTop.result.name}`}).check();
 await panel.getByRole('button',{name:'生成计划'}).click();
 await expect(panel.getByRole('status').filter({hasText:'计划已生成并保存'})).toBeVisible({timeout:20000});
 const days=panel.locator('.scenario-day');
 await expect(days).toHaveCount(3);
 const dayItemsWithRole=async(index:number,role:string)=>{
  const items=days.nth(index).locator('.scenario-day-items li');
  const total=await items.count();
  const found:string[]=[];
  for(let position=0;position<total;position++){
   const text=await items.nth(position).textContent();
   if(text&&text.includes(role))found.push(text);
  }
  return found.sort().join('|');
 };
 for(let index=0;index<3;index++){
  const dayText=await days.nth(index).textContent();
  expect(dayText).not.toContain('备用衬衫','被排除单品不出现');
  expect((await dayItemsWithRole(index,'鞋')).length).toBeGreaterThan(0,'每天都要有鞋');
  expect(dayText).toContain('方案完整','真实库存足够时逐天方案完整');
 }
 // 贴身/主要衣物不连续两天重复：第2天的下装必须换一条。
 expect(await dayItemsWithRole(1,'下装')).not.toBe(await dayItemsWithRole(0,'下装'));
 // 打包清单去重：鞋跨天复用只计一次，重复使用如实标记。
 const packing=panel.locator('.scenario-packing-list li');
 const dayItemCount=await days.nth(0).locator('.scenario-day-items li').count()
  +await days.nth(1).locator('.scenario-day-items li').count()
  +await days.nth(2).locator('.scenario-day-items li').count();
 const packingTotal=await packing.count();
 expect(packingTotal).toBeGreaterThan(0);
 expect(packingTotal).toBeLessThan(dayItemCount,'打包清单必须按衣物去重');
 await expect(panel.locator('.scenario-packing-list')).toContainText('旅行中重复使用');

 // 保存某一天为穿搭模板：模板不记穿着。
 await days.nth(0).getByRole('button',{name:'保存这一天为穿搭模板'}).click();
 await expect(panel.getByRole('status').filter({hasText:'已保存为穿搭模板'})).toBeVisible();
 const wears=await api('wear.list',{member_id:member});
 expect(wears.result.items.length).toBe(wearsBaseline);
 const outfits=await api('outfit.list',{member_id:member});
 const names=(outfits.result.items as {name:string}[]).map(item=>item.name);
 expect(names.some(name=>name.includes('三日出行'))).toBe(true);

 // 修改计划：排除西裤 → 重算后西裤消失、版本递增。
 await panel.locator('.scenario-update summary').click();
 await panel.getByRole('checkbox',{name:`修改时排除 ${suitBottom.result.name}`}).check();
 await panel.getByRole('button',{name:'按新约束重算并保存'}).click();
 await expect(panel.getByRole('status').filter({hasText:'计划已按新约束重算保存'})).toBeVisible({timeout:20000});
 const updatedDays=panel.locator('.scenario-day');
 await expect(updatedDays).toHaveCount(3);
 for(let index=0;index<3;index++){
  expect(await updatedDays.nth(index).textContent()).not.toContain('西裤');
 }
 // 已保存计划列表可回看。
 await panel.getByRole('button',{name:'刷新计划列表'}).click();
 await expect(panel.locator('.scenario-plan-list')).toContainText('三日出行');
 // 端到端后仍无新增穿着记录。
 const wearsAfter=await api('wear.list',{member_id:member});
 expect(wearsAfter.result.items.length).toBe(wearsBaseline);
});

test('scenario tab reports shortage honestly when the whole pool is excluded',async({page:browserPage})=>{
 page=browserPage;
 const panel=await openScenarios(browserPage);
 const space=await api('space.default',{});
 const member=space.result.member_id as string;
 await api('clothing.create',{member_id:member,name:'无解旅行对照上衣',category:'top'});
 // 共享 dev server 上默认成员已有历史穿着：断言“排除全部后不写穿着”用增量。
 const wearsBaseline=(await api('wear.list',{member_id:member})).result.items.length;
 // 排除当前库存中的全部可用单品 → 池为空，方案必须如实说明且不虚构。
 await panel.getByRole('button',{name:'刷新计划列表'}).click();
 await panel.locator('.scenario-items summary').click();
 const boxes=panel.locator('.scenario-stock input[aria-label^="排除"]');
 await expect(boxes.first()).toBeVisible({timeout:20000});
 const count=await boxes.count();
 expect(count).toBeGreaterThan(0);
 for(let index=0;index<count;index++){
  await boxes.nth(index).check();
 }
 await panel.getByRole('button',{name:'多日旅行',exact:true}).click();
 await panel.getByLabel('旅行天数').fill('2');
 await panel.getByLabel('标题（可选）').fill('无解旅行');
 await panel.getByRole('button',{name:'生成计划'}).click();
 await expect(panel.locator('.scenario-shortage')).toBeVisible({timeout:20000});
 await expect(panel.locator('.scenario-shortage')).toContainText('缺少');
 await expect(panel.locator('.scenario-shortage')).toContainText('没有满足成员、生命周期、洗护及排除条件的真实衣物');
 await expect(panel.locator('.scenario-day').first()).toContainText('缺少');
 const wears=await api('wear.list',{member_id:member});
 expect(wears.result.items.length).toBe(wearsBaseline);
});

test('scenario planning stays usable at 320px width with keyboard focus',async({page:browserPage})=>{
 page=browserPage;
 await browserPage.setViewportSize({width:320,height:800});
 const panel=await openScenarios(browserPage);
 // 关键控件键盘可达：原生 input/button 可聚焦，无 hover 依赖。
 const dateInput=panel.getByLabel('开始日期');
 await expect(dateInput).toBeEnabled({timeout:20000});
 await dateInput.focus();
 await expect(dateInput).toBeFocused();
 await panel.getByRole('button',{name:'生成计划'}).focus();
 await expect(panel.getByRole('button',{name:'生成计划'})).toBeFocused();
 // 320 宽度无横向溢出。
 const overflow=await browserPage.evaluate(()=>document.documentElement.scrollWidth>document.documentElement.clientWidth+1);
 expect(overflow).toBe(false);
});
