import {test,expect} from '@playwright/test';
// 衣橱洞察验收（新 UI）：记录 → 衣橱洞察；统计设置在“统计设置与口径”详情内，
// 指标卡以“衣橱单品/实际穿着/尚未穿过”呈现。手算口径、阈值、更正撤销重算、
// 加载门控与成员隔离断言保持不变。
test.afterEach(async({request})=>{const space=await request.post('/__wardrobe_dev_api',{data:{operation:'space.default',input:{}}});const body=await space.json();await request.post('/__wardrobe_dev_api',{data:{operation:'member.select',input:{member_id:body.result.member_id}}});});
const nav=(page:any,name:string)=>page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name,exact:true});
async function openInsights(page:any){
 await nav(page,'记录').click();
 await page.getByRole('button',{name:'衣橱洞察',exact:true}).click();
 const panel=page.locator('#insights');
 return panel;
}
const metric=(panel:any,label:string)=>panel.locator('.journal-metrics article').filter({hasText:label}).locator('strong');
test('洞察手算分布、次数天数、90天边界、成员偏好重载及更正撤销重算',async({page})=>{
 await page.goto('/');
 const panel=await openInsights(page);
 await expect(panel.locator('.journal-metrics')).toBeVisible({timeout:5000});
 const execute=async(operation:string,input:object)=>{const r=await page.request.post('/__wardrobe_dev_api',{data:{operation,input}});const body=await r.json();expect(body.error).toBeUndefined();return body.result;};
 const original=(await execute('space.default',{})).member_id;const m=await execute('member.create',{name:'洞察手算验收'});const member=m.member_id;const ids:string[]=[];
 for(const [name,category,color]of [['A','top',['red','blue']],['B','top',['red']],['C','bottom',[]],['D','outerwear',['green']],['E','other',[]],['F','shoes',[]]] as [string,string,string[]][]){ids.push((await execute('clothing.create',{member_id:member,name,category,color})).id);}
 const events=[];for(const[key,date,clothing_ids]of [['ab1','2020-01-01',[ids[0],ids[1]]],['ab2','2020-01-01',[ids[1],ids[0]]],['af','2020-01-02',[ids[0],ids[5]]],['d','2020-01-03',[ids[3]]]] as [string,string,string[]][]){events.push(await execute('wear.record',{member_id:member,operation_key:key,local_date:date,timezone:'Asia/Shanghai',clothing_ids}));}
 await execute('clothing.archive',{member_id:member,id:ids[3]});await execute('clothing.delete',{member_id:member,id:ids[5]});
 await page.reload();
 await page.getByText('当前衣橱',{exact:true}).click();
 await page.getByRole('combobox',{name:'当前成员',exact:true}).selectOption(member);
 await expect(page.getByRole('combobox',{name:'当前成员',exact:true})).toBeEnabled();
 await expect(page.getByRole('combobox',{name:'当前成员',exact:true})).toHaveValue(member);
 await openInsights(page);
 await panel.locator('summary').filter({hasText:'统计设置与口径'}).click();
 await expect(panel.getByLabel('久未穿天数')).toHaveValue('90');
 await panel.getByLabel('统计截止日期').fill('2020-04-01');
 await panel.getByLabel('统计时区').fill('Asia/Shanghai');
 await panel.getByRole('button',{name:'重新计算洞察'}).click();
 const result=await execute('insights.summary',{member_id:member,as_of_date:'2020-04-01',timezone:'Asia/Shanghai'});expect(result.inventory.total).toBe(5);expect(result.stale).toHaveLength(2);expect(result.never_worn).toHaveLength(2);expect(result.effective_event_count).toBe(4);
 await expect(metric(panel,'衣橱单品')).toHaveText('5 件');
 await expect(panel.getByRole('heading',{name:'类别分布'})).toBeVisible();
 await expect(panel.getByText('红 · 2 件 / 40%',{exact:true})).toBeVisible();
 await expect(panel.getByText('未设颜色标签 · 2 件 / 40%',{exact:true})).toBeVisible();
 await expect(panel.getByText('A · 3 次 / 2 天',{exact:true})).toBeVisible();
 await expect(panel.getByRole('heading',{name:'久未穿 · 2 件'})).toBeVisible();
 await expect(panel.getByRole('heading',{name:'从未穿过 · 2 件'})).toBeVisible();
 await expect(panel.getByText('A + B · 2 次 / 1 天',{exact:true})).toBeVisible();
 for(const width of [320,375,414,768,1024,1280,1440]){await page.setViewportSize({width,height:1000});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBeTruthy();if(width===320||width===1280)await panel.screenshot({path:`../../docs/verification/ticket-11/insights-${width}.png`});}
 await panel.getByLabel('久未穿天数').fill('89');
 await panel.getByRole('button',{name:'保存久未穿阈值'}).click();
 await expect(panel.getByRole('status').filter({hasText:'阈值已保存'})).toBeVisible();
 await expect(panel.getByRole('heading',{name:'久未穿 · 3 件'})).toBeVisible();
 await page.reload();
 await openInsights(page);
 await panel.locator('summary').filter({hasText:'统计设置与口径'}).click();
 await expect(panel.getByLabel('久未穿天数')).toHaveValue('89');
 await panel.getByLabel('统计截止日期').fill('2020-04-01');
 await panel.getByLabel('统计时区').fill('Asia/Shanghai');
 await execute('wear.correct',{member_id:member,root_event_id:events[2].id,expected_revision:1,operation_key:'correct-af',local_date:'2020-01-03',timezone:'Asia/Shanghai'});
 await execute('wear.revoke',{member_id:member,root_event_id:events[1].id,expected_revision:1,operation_key:'revoke-ab2',reason:'误记'});
 await panel.getByRole('button',{name:'重新计算洞察'}).click();
 await expect(panel.getByText('A · 2 次 / 2 天',{exact:true})).toBeVisible();
 await expect(panel.getByText('A + B · 1 次 / 1 天',{exact:true})).toBeVisible();
 await panel.getByRole('button',{name:'恢复默认 90 天'}).click();
 await expect(panel.getByLabel('久未穿天数')).toHaveValue('90');
 await expect(panel.getByRole('heading',{name:'久未穿 · 1 件'})).toBeVisible();
 await page.getByText('当前衣橱',{exact:true}).click();
 await page.getByRole('combobox',{name:'当前成员',exact:true}).selectOption(original);
 await expect(page.getByRole('combobox',{name:'当前成员',exact:true})).toBeEnabled();
 await expect(page.getByRole('combobox',{name:'当前成员',exact:true})).toHaveValue(original);
 await openInsights(page);
 await panel.locator('summary').filter({hasText:'统计设置与口径'}).click();
 await expect(panel.getByLabel('久未穿天数')).toHaveValue('90');
});
test('洞察空态、真实查询加载、参数错误与修改恢复都有明确状态',async({page})=>{
 await page.goto('/');const r=await page.request.post('/__wardrobe_dev_api',{data:{operation:'member.create',input:{name:'洞察空态验收'}}});const m=(await r.json()).result;await page.reload();
 await page.getByText('当前衣橱',{exact:true}).click();
 await page.getByRole('combobox',{name:'当前成员',exact:true}).selectOption(m.member_id);
 await expect(page.getByRole('combobox',{name:'当前成员',exact:true})).toBeEnabled();
 await expect(page.getByRole('combobox',{name:'当前成员',exact:true})).toHaveValue(m.member_id);
 await page.route('**/__wardrobe_dev_api',async route=>{if(route.request().postDataJSON().operation==='insights.summary')await new Promise(resolve=>setTimeout(resolve,500));await route.continue();});
 const panel=await openInsights(page);
 await panel.locator('summary').filter({hasText:'统计设置与口径'}).click();
 await expect(panel.getByRole('status').filter({hasText:'正在重算洞察'})).toBeVisible();
 await expect(page.getByRole('combobox',{name:'当前成员',exact:true})).toBeDisabled();
 await expect(metric(panel,'衣橱单品')).toHaveText('0 件');
 await expect(panel.getByText('暂无实际组合。保存模板不会生成组合统计。',{exact:true})).toBeVisible();
 await panel.getByLabel('统计截止日期').fill('2999-01-01');await panel.getByRole('button',{name:'重新计算洞察'}).click();await expect(panel.getByRole('alert')).toContainText('有效统计日期');await panel.getByLabel('统计截止日期').fill('2020-01-01');await panel.getByRole('button',{name:'重新计算洞察'}).click();await expect(panel.getByRole('alert')).toHaveCount(0);await panel.getByLabel('久未穿天数').fill('0');await panel.getByRole('button',{name:'保存久未穿阈值'}).click();await expect(panel.getByRole('alert')).toContainText('1–36500');await panel.getByLabel('久未穿天数').fill('91');await panel.getByRole('button',{name:'保存久未穿阈值'}).click();await expect(panel.getByRole('status').filter({hasText:'阈值已保存'})).toBeVisible();await expect(panel.getByLabel('久未穿天数')).toHaveValue('91');
 await panel.getByRole('button',{name:'重新计算洞察'}).focus();await expect(panel.getByRole('button',{name:'重新计算洞察'})).toBeFocused();
});
test('重载时提前打开洞察会等待衣物加载后读取已保存阈值',async({page})=>{const call=async(operation:string,input:object)=>{const response=await page.request.post('/__wardrobe_dev_api',{data:{operation,input}});const body=await response.json();expect(body.error).toBeUndefined();return body.result;};const member=(await call('member.create',{name:'洞察延迟加载'})).member_id;await call('insights.configure',{member_id:member,stale_days:89,expected_revision:0});await call('member.select',{member_id:member});let release!:()=>void;const gate=new Promise<void>(r=>release=r);let blocked=false,summaries=0;await page.route('**/__wardrobe_dev_api',async route=>{if(route.request().postDataJSON().operation==='insights.summary')summaries++;if(route.request().postDataJSON().operation==='clothing.list'){const response=await route.fetch();blocked=true;await gate;await route.fulfill({response});return;}await route.continue();});await page.goto('/');await expect.poll(()=>blocked).toBe(true);await expect(page.locator('.member-switcher select')).toHaveValue(member);
 // 面板按需挂载：先进入衣橱洞察（此时衣物列表仍被扣住，洞察等待衣物加载）。
 await page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name:'记录',exact:true}).click();
 await page.getByRole('button',{name:'衣橱洞察',exact:true}).click();
 const panel=page.locator('#insights');await panel.locator('summary').filter({hasText:'统计设置与口径'}).click();await expect(panel.getByLabel('久未穿天数')).toBeDisabled();release();await expect(panel.getByLabel('久未穿天数')).toHaveValue('89');await expect(metric(panel,'衣橱单品')).toHaveText('0 件');await expect(panel.getByRole('status').filter({hasText:'正在重算洞察'})).toHaveCount(0);await page.waitForTimeout(150);expect(summaries).toBe(1);});

test('成员切换真实回执延迟时旧洞察只读，确认切换后新成员重新展开读取',async({page})=>{
 await page.goto('/');
 const execute=async(operation:string,input:object)=>{const response=await page.request.post('/__wardrobe_dev_api',{data:{operation,input}});const body=await response.json();expect(body.error).toBeUndefined();return body.result;};
 const a=(await execute('member.create',{name:'洞察延迟旧成员'})).member_id;
 const b=(await execute('member.create',{name:'洞察延迟新成员'})).member_id;
 for(const member of [a,a,b])await execute('clothing.create',{member_id:member,name:'delayed scope fixture',category:'top'});
 await page.reload();
 await page.getByText('当前衣橱',{exact:true}).click();
 const select=page.getByRole('combobox',{name:'当前成员',exact:true});
 await select.selectOption(a);await expect(select).toBeEnabled();await expect(select).toHaveValue(a);
 const panel=await openInsights(page);
 await expect(metric(panel,'衣橱单品')).toHaveText('2 件');await panel.locator('summary').filter({hasText:'统计设置与口径'}).click();
 let enter!:()=>void,release!:()=>void;const entered=new Promise<void>(resolve=>enter=resolve),gate=new Promise<void>(resolve=>release=resolve);
 await page.route('**/__wardrobe_dev_api',async route=>{
  const body=route.request().postDataJSON();
  if(body.operation==='member.select'&&body.input.member_id===b){const actual=await route.fetch();enter();await gate;await route.fulfill({response:actual});}else await route.continue();
 });
 try{
  await select.selectOption(b);await entered;
  await expect(select).toBeDisabled();
  await panel.locator('summary').filter({hasText:'统计设置与口径'}).click();
  await expect(panel.getByLabel('统计截止日期')).toBeDisabled();await expect(metric(panel,'衣橱单品')).toHaveText('2 件');
  release();await expect(select).toBeEnabled();await expect(select).toHaveValue(b);
  // 切换完成后应用重置回衣橱工作区：旧洞察面板卸载，迟到响应不落地。
  await expect(panel.getByLabel('统计截止日期')).toHaveCount(0,{timeout:5000}).catch(async()=>{await expect(panel.getByLabel('统计截止日期')).toBeHidden();});
  // 重新进入衣橱洞察：新成员面板重新加载自己的数据。
  await nav(page,'记录').click();
  await page.getByRole('button',{name:'衣橱洞察',exact:true}).click();
  const fresh=page.locator('#insights');
  await expect(metric(fresh,'衣橱单品')).toHaveText('1 件');
  await fresh.locator('summary').filter({hasText:'统计设置与口径'}).click();
  await expect(fresh.getByLabel('统计截止日期')).toBeEnabled();
 }finally{release();await page.unroute('**/__wardrobe_dev_api');}
});
