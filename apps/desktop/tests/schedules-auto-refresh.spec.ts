import {test,expect} from '@playwright/test';
// GUI任务状态刷新验收：后台（原生调度或CLI）完成到期计划后，处于打开状态的
// 定时计划面板必须在数秒内自动反映执行记录与通知提交状态，
// 不要求用户重新点击“检查到期计划”或重开页面。后端为真实CLI桥，非mock。
test('后台完成定时任务后打开中的面板自动刷新执行与通知状态',async({page})=>{
 test.setTimeout(240000);
 const call=async(operation:string,input:object)=>{
  const response=await page.request.post('/__wardrobe_dev_api',{data:{operation,input}});
  const body=await response.json();expect(body.error).toBeUndefined();return body.result;
 };
 // 计划本地时间取下一分钟边界：创建不追溯，等时刻真实过后登记即到期。
 const zone='Asia/Shanghai';
 const dueLocal=new Intl.DateTimeFormat('en-GB',{timeZone:zone,hour:'2-digit',minute:'2-digit',hour12:false}).format(new Date(Date.now()+65_000));
 const device=(await call('schedule.device',{})).id;
 const plan=await call('schedule.create',{member_id:'default',name:'后台完成自动刷新计划',kind:'care',local_time:dueLocal,timezone:zone,weekdays:[1,2,3,4,5,6,7],execution_device_id:device,occasion:'daily',target_day_offset:0,enabled:true});
 await page.goto('/');
 await page.getByText('当前衣橱',{exact:true}).click();
 await expect(page.getByRole('combobox',{name:'当前成员',exact:true})).toBeEnabled({timeout:30000});
 await page.getByRole('button',{name:'设置',exact:true}).click();
 const card=page.locator('.settings-hub').getByText('定时计划与通知',{exact:true});
 await card.click();
 // 打开计划详情与执行记录：此刻应为空。
 await page.locator('#schedules').getByText('定时计划与通知',{exact:true}).click();
 const row=page.getByRole('listitem').filter({hasText:'后台完成自动刷新计划'});
 await expect(row).toBeVisible();
 await row.getByRole('button',{name:'查看执行记录',exact:true}).click();
 await expect(page.getByLabel('计划执行记录')).toContainText('还没有执行记录');
 await expect(page.getByText('尚无通知提交。系统通知接收不等于已经显示或阅读。',{exact:true})).toBeVisible();
 // 等计划本地时刻真实过去，再经真实CLI登记并完成到期任务（等同后台完成）。
 await page.waitForFunction((due)=>{const now=new Intl.DateTimeFormat('en-GB',{timeZone:'Asia/Shanghai',hour:'2-digit',minute:'2-digit',hour12:false}).format(new Date());return now>due;},dueLocal);
 const ticked=await call('schedule.tick',{member_id:'default'});
 expect(ticked.queued.length,'到期任务应已登记').toBeGreaterThan(0);
 const jobId=ticked.queued[0].job_id;
 const done=await call('schedule.worker.run',{member_id:'default',id:jobId});
 expect(done.state).toBe('succeeded');
 // 不做任何界面操作：打开中的面板应在数秒内自动显示已完成与等待本机提交。
 await expect(page.getByLabel('计划执行记录')).toContainText('已完成',{timeout:15000});
 await expect(page.getByText('等待本机提交',{exact:false})).toBeVisible();
 await expect(page.getByText('尚无通知提交。系统通知接收不等于已经显示或阅读。',{exact:true})).toHaveCount(0);
});
