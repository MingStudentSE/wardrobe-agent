import {test,expect} from '@playwright/test';
import {restoreDefaultMember} from './support/member-hygiene';
// GUI竞态验收（切页/成员切换/卸载）：定时计划面板在成员切换后不得残留上一成员的
// 计划或编辑状态；迟到的上一成员响应不得写入已卸载的视图；切页期间进行中的到期
// 检查完成后状态仍回到本面板。后端为真实CLI桥，非mock。
const call=async(page:import('@playwright/test').Page,operation:string,input:object)=>{
 const response=await page.request.post('/__wardrobe_dev_api',{data:{operation,input}});
 const body=await response.json();expect(body.error).toBeUndefined();return body.result;
};
// 侧栏身份区默认折叠；已展开时不能再次点击（会折叠），先探测再展开。
const openIdentity=async(page:import('@playwright/test').Page)=>{
 const box=page.getByRole('combobox',{name:'当前成员',exact:true});
 if(!await box.isVisible().catch(()=>false))await page.getByText('当前衣橱',{exact:true}).click();
};
// 设置目录停留在本页时点击设置即可回到面板；在总览时点击目录卡片；面板详情默认折叠需展开。
const openSchedules=async(page:import('@playwright/test').Page)=>{
 await page.getByRole('button',{name:'设置',exact:true}).click();
 const card=page.locator('.settings-hub').getByText('定时计划与通知',{exact:true});
 if(await card.isVisible().catch(()=>false))await card.click();
 const summary=page.locator('#schedules').getByText('定时计划与通知',{exact:true});
 if(await summary.isVisible().catch(()=>false))await summary.click();
};
test('成员切换不残留计划面板编辑状态且迟到响应不串成员',async({page})=>{
 test.setTimeout(240000);
 const device=(await call(page,'schedule.device',{})).id;
 const planInput=(name:string,member:string)=>({member_id:member,name,kind:'recommendation',local_time:'23:50',timezone:'Asia/Shanghai',weekdays:[1,2,3,4,5,6,7],execution_device_id:device,occasion:'daily',target_day_offset:1,enabled:true});
 const defaultMember=(await call(page,'space.default',{})).member_id;
 await call(page,'schedule.create',planInput('切换前甲成员计划',defaultMember));
 const second=(await call(page,'member.create',{name:'计划竞态乙成员'})).member_id;
 await call(page,'schedule.create',planInput('切换后乙成员计划',second));
 await page.goto('/');
 await openIdentity(page);
 await expect(page.getByRole('combobox',{name:'当前成员',exact:true})).toBeEnabled({timeout:30000});
 await openSchedules(page);
 await expect(page.getByRole('listitem').filter({hasText:'切换前甲成员计划'})).toBeVisible();
 // 进入修改状态后直接切换成员：表单必须回到新建空表单，且只显示新成员的计划。
 await page.getByRole('listitem').filter({hasText:'切换前甲成员计划'}).getByRole('button',{name:'修改计划',exact:true}).click();
 await expect(page.getByRole('heading',{name:'修改未来计划'})).toBeVisible();
 await expect(page.getByLabel('计划名称',{exact:true})).toHaveValue('切换前甲成员计划');
 await openIdentity(page);
 await page.getByRole('combobox',{name:'当前成员',exact:true}).selectOption(second);
 await expect(page.getByRole('heading',{name:'我的衣橱'})).toBeVisible();
 await openSchedules(page);
 await expect(page.getByRole('listitem').filter({hasText:'切换后乙成员计划'})).toBeVisible();
 await expect(page.getByRole('listitem').filter({hasText:'切换前甲成员计划'})).toHaveCount(0);
 await expect(page.getByRole('heading',{name:'安排下一次准备'})).toBeVisible();
 await expect(page.getByLabel('计划名称',{exact:true})).toHaveValue('');
 // 切回甲成员：同样只显示甲成员计划，编辑状态不跨成员残留。
 await openIdentity(page);
 await page.getByRole('combobox',{name:'当前成员',exact:true}).selectOption(defaultMember);
 await expect(page.getByRole('heading',{name:'我的衣橱'})).toBeVisible();
 await openSchedules(page);
 await expect(page.getByRole('listitem').filter({hasText:'切换前甲成员计划'})).toBeVisible();
 await expect(page.getByRole('listitem').filter({hasText:'切换后乙成员计划'})).toHaveCount(0);
 // 重载并拦住甲成员的schedule.list迟到放行：切到乙成员后不得串回甲成员计划。
 let release!:()=>void;const gated=new Promise<void>(resolve=>release=resolve);
 let holding=false;
 await page.route('**/__wardrobe_dev_api',async route=>{
  const request=route.request();
  const body=request.postDataJSON?.()??{};
  if(holding&&request.method()==='POST'&&body?.operation==='schedule.list'&&body?.input?.member_id==='default'){
   holding=false;const response=await route.fetch();await gated;await route.fulfill({response});return;
  }
  await route.continue();
 });
 holding=true;
 await page.reload();
 await page.getByText('当前衣橱',{exact:true}).click();
 await expect(page.getByRole('combobox',{name:'当前成员',exact:true})).toBeEnabled({timeout:30000});
 await openSchedules(page);
 // 甲成员的计划读取被扣住，面板停留在读取中。
 await expect(page.getByRole('status').filter({hasText:'正在读取计划'})).toBeVisible();
 await openIdentity(page);
 await page.getByRole('combobox',{name:'当前成员',exact:true}).selectOption(second);
 await expect(page.getByRole('heading',{name:'我的衣橱'})).toBeVisible();
 await openSchedules(page);
 await expect(page.getByRole('listitem').filter({hasText:'切换后乙成员计划'})).toBeVisible();
 release();
 await page.waitForTimeout(500);
 await expect(page.getByRole('listitem').filter({hasText:'切换后乙成员计划'})).toBeVisible();
 await expect(page.getByRole('listitem').filter({hasText:'切换前甲成员计划'})).toHaveCount(0);
});
test('到期检查进行中切换设置页不丢失完成状态也不留错误',async({page})=>{
 test.setTimeout(240000);
 // 同一运行共享数据目录：计划建在当前选定成员上，不假设default仍被选中。
 const space=await call(page,'space.default',{});
 const selected=space.selected_member_id??space.member_id;
 await call(page,'schedule.create',{member_id:selected,name:'切页到期检查计划',kind:'care',local_time:'23:50',timezone:'Asia/Shanghai',weekdays:[1,2,3,4,5,6,7],execution_device_id:(await call(page,'schedule.device',{})).id,occasion:'daily',target_day_offset:0,enabled:true});
 await page.goto('/');
 await openSchedules(page);
 await expect(page.getByRole('listitem').filter({hasText:'切页到期检查计划'})).toBeVisible();
 let release!:()=>void;const gated=new Promise<void>(resolve=>release=resolve);
 let holding=false;
 await page.route('**/__wardrobe_dev_api',async route=>{
  const request=route.request();
  const body=request.postDataJSON?.()??{};
  if(holding&&request.method()==='POST'&&body?.operation==='schedule.tick'){
   holding=false;const response=await route.fetch();await gated;await route.fulfill({response});return;
  }
  await route.continue();
 });
 holding=true;
 await page.getByRole('button',{name:'检查到期计划',exact:true}).click();
 // 进行中切到设置目录其他页再切回来，随后放行到期检查响应。
 await page.getByRole('button',{name:'← 返回设置'}).click();
 await page.locator('.settings-hub').getByText('备份与恢复',{exact:true}).click();
 await expect(page.locator('#backup-settings')).toBeVisible();
 await page.getByRole('button',{name:'← 返回设置'}).click();
 await page.locator('.settings-hub').getByText('定时计划与通知',{exact:true}).click();
 release();
 await expect(page.getByRole('status').filter({hasText:'已登记本成员到期任务'})).toBeVisible();
 await expect(page.locator('#schedules [role="alert"]')).toHaveCount(0);
});
test.afterEach(async({request})=>{await restoreDefaultMember(request);});
