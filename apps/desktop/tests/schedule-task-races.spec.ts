import {test,expect} from '@playwright/test';
// GUI竞态验收（成员切换/任务状态刷新/切页）：任务中心“刷新状态”的迟到响应
// 不得把上一成员的定时任务渲染到已切换成员的视图。后端为真实CLI桥，非mock。
test('成员切换后任务中心不回写上一成员的定时任务',async({page})=>{
 test.setTimeout(240000);
 const call=async(operation:string,input:object)=>{
  const response=await page.request.post('/__wardrobe_dev_api',{data:{operation,input}});
  const body=await response.json();expect(body.error).toBeUndefined();return body.result;
 };
 // 计划本地时间取下一分钟边界（Asia/Shanghai）：创建不追溯，等时刻真实过后tick才登记任务。
 const zone='Asia/Shanghai';
 const format=(at:number)=>new Intl.DateTimeFormat('en-GB',{timeZone:zone,hour:'2-digit',minute:'2-digit',hour12:false}).format(new Date(at));
 const dueLocal=format(Date.now()+65_000);
 const device=(await call('schedule.device',{})).id;
 const plan=await call('schedule.create',{member_id:'default',name:'任务刷新竞态计划',kind:'care',local_time:dueLocal,timezone:zone,weekdays:[1,2,3,4,5,6,7],execution_device_id:device,occasion:'daily',target_day_offset:0,enabled:true});
 expect(plan.enabled).toBe(true);
 const second=(await call('member.create',{name:'任务竞态乙成员'})).member_id;
 await page.goto('/');
 await page.getByText('当前衣橱',{exact:true}).click();
 const switcher=page.getByRole('combobox',{name:'当前成员',exact:true});
 await expect(switcher).toBeEnabled({timeout:30000});
 await page.getByRole('button',{name:'设置',exact:true}).click();
 await page.locator('.settings-hub').getByText('任务中心',{exact:true}).click();
 // 等待计划本地时刻真实过去，再经真实CLI登记到期任务。
 await page.waitForFunction((due)=>{const now=new Intl.DateTimeFormat('en-GB',{timeZone:'Asia/Shanghai',hour:'2-digit',minute:'2-digit',hour12:false}).format(new Date());return now>due;},dueLocal);
 const ticked=await call('schedule.tick',{member_id:'default'});
 expect(ticked.queued.length,'到期任务应已登记').toBeGreaterThan(0);
 await expect(page.getByText('衣橱任务',{exact:true})).toBeVisible({timeout:15000});
 // 从现在起拦住default成员的jobs.list并迟到放行；乙成员请求照常直通。
 let release!:()=>void;const gated=new Promise<void>(resolve=>release=resolve);
 let holding=false;
 await page.route('**/__wardrobe_dev_api',async route=>{
  const request=route.request();
  const body=request.postDataJSON?.()??{};
  if(holding&&request.method()==='POST'&&body?.operation==='jobs.list'&&body?.input?.member_id==='default'){
   holding=false;const response=await route.fetch();await gated;await route.fulfill({response});return;
  }
  await route.continue();
 });
 holding=true;
 await page.getByRole('button',{name:'刷新状态',exact:true}).click();
 // 切换成员：应用异步回到衣橱视图，设置目录停留在任务中心；乙成员没有任务。
 await switcher.selectOption(second);
 await expect(page.getByRole('heading',{name:'我的衣橱'})).toBeVisible();
 await page.getByRole('button',{name:'设置',exact:true}).click();
 await expect(page.locator('.task-center')).toBeVisible();
 await expect(page.getByText('目前没有后台任务')).toBeVisible();
 // 迟到的上一成员响应到达后，当前成员视图必须仍是空任务状态。
 release();
 await page.waitForTimeout(500);
 await expect(page.getByText('目前没有后台任务')).toBeVisible();
 await expect(page.getByText('衣橱任务',{exact:true})).toHaveCount(0);
});
