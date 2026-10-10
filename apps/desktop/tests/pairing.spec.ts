import {test,expect} from '@playwright/test';
// Ticket19 pairing acceptance through the real public app: a visible
// single-item anchor selection calls pairing.generate, every proposal keeps
// the anchor, and the durable member-scoped history survives a reload via
// pairing.list. Same harness contract as outfit-studio.spec.ts.
test('visible anchor selection generates pairing intents and durable history',async({page})=>{
 const call=async(operation:string,input:object)=>{const r=await (await page.request.post('/__wardrobe_dev_api',{data:{operation,input}})).json();if(r.error)throw Error(r.error.message);return r.result;};
 await call('clothing.create',{member_id:'default',name:'配对锚点外套',category:'top'});
 await call('clothing.create',{member_id:'default',name:'配对下装',category:'bottom'});
 await call('clothing.create',{member_id:'default',name:'配对鞋',category:'shoes'});
 await page.goto('/');
 await expect(page.locator('.collection .grid article').first()).toBeVisible();
 await page.getByRole('navigation').getByRole('button',{name:'穿搭',exact:true}).click();
 await page.getByRole('button',{name:'单品配对',exact:true}).click();
 const studio=page.locator('#pairing');
 await studio.getByLabel('选择锚点单品').selectOption({label:'配对锚点外套'});
 await studio.getByRole('button',{name:'为锚点搭配',exact:true}).click();
 await expect(studio.locator('.pairing-proposal').first()).toBeVisible();
 const proposals=studio.locator('.pairing-proposal');
 const count=await proposals.count();
 expect(count).toBeGreaterThanOrEqual(1);
 for(let index=0;index<count;index++){
  await expect(proposals.nth(index)).toContainText('配对锚点外套');
 }
 // The durable pairing history is readable again after a full reload.
 await page.reload();
 await page.getByRole('navigation').getByRole('button',{name:'穿搭',exact:true}).click();
 await page.getByRole('button',{name:'单品配对',exact:true}).click();
 await expect(page.locator('#pairing').locator('.pairing-history-item').first()).toContainText('配对锚点外套');
});
// Save through the shared template seam, then a like plus the visible
// recompute control must change the next anchored generation (taught pair
// first), proving pairing feedback reaches shared learning. The oracle runs
// on an isolated member inventory so the shared seeded database cannot add
// competing candidates.
test('pairing save feedback and recompute change the next anchored generation',async({page})=>{
 const call=async(operation:string,input:object)=>{const r=await (await page.request.post('/__wardrobe_dev_api',{data:{operation,input}})).json();if(r.error)throw Error(r.error.message);return r.result;};
 const member=(await call('member.create',{name:'教学成员'})).member_id as string;
 await call('clothing.create',{member_id:member,name:'教学锚点上衣',category:'top'});
 await call('clothing.create',{member_id:member,name:'教学下装甲',category:'bottom'});
 await call('clothing.create',{member_id:member,name:'教学下装乙',category:'bottom'});
 await call('clothing.create',{member_id:member,name:'教学鞋',category:'shoes'});
 await page.goto('/');
 await expect(page.locator('.collection .grid article').first()).toBeVisible();
 await page.getByText('当前衣橱').click();
 await page.getByRole('combobox',{name:/^当前成员/}).selectOption({label:'教学成员'});
 await expect(page.locator('.collection').getByText('教学锚点上衣')).toBeVisible();
 await page.getByRole('navigation').getByRole('button',{name:'穿搭',exact:true}).click();
 await page.getByRole('button',{name:'单品配对',exact:true}).click();
 const studio=page.locator('#pairing');
 await studio.getByLabel('选择锚点单品').selectOption({label:'教学锚点上衣'});
 await studio.getByRole('button',{name:'为锚点搭配',exact:true}).click();
 const proposals=studio.locator('.pairing-proposal');
 await expect(proposals).toHaveCount(2);
 const secondText=await proposals.nth(1).innerText();
 const taught=secondText.includes('教学下装甲')?'教学下装甲':'教学下装乙';
 await proposals.nth(0).getByLabel('搭配保存名称').fill('配对保存验收');
 await proposals.nth(0).getByRole('button',{name:'保存为搭配',exact:true}).click();
 await expect(studio.getByRole('status').filter({hasText:'已保存到穿搭库'})).toBeVisible();
 const outfits=await call('outfit.list',{member_id:member});
 expect(JSON.stringify(outfits)).toContain('配对保存验收');
 await studio.getByLabel('反馈方案').selectOption('1');
 await studio.getByRole('button',{name:'喜欢',exact:true}).click();
 await expect(studio.getByRole('status').filter({hasText:'没有新增穿着'})).toBeVisible();
 await studio.getByRole('button',{name:'重新计算学习偏好'}).click();
 await expect(studio.getByRole('status').filter({hasText:'偏好已重新计算'})).toBeVisible();
 await studio.getByRole('button',{name:'为锚点搭配',exact:true}).click();
 await expect(proposals.first()).toContainText(taught);
});
// A pairing load that is still in flight across a member switch must never
// surface the previous member's garments, anchor or history, and the fresh
// member keeps her own anchor flow and durable history.
test('member switch mid pairing load never renders stale cross member results',async({page})=>{
 const call=async(operation:string,input:object)=>{const r=await (await page.request.post('/__wardrobe_dev_api',{data:{operation,input}})).json();if(r.error)throw Error(r.error.message);return r.result;};
 const space=await call('space.default',{});
 const memberA=space.member_id as string;
 await call('clothing.create',{member_id:'default',name:'甲锚点上衣',category:'top'});
 await call('clothing.create',{member_id:'default',name:'甲下装',category:'bottom'});
 await call('clothing.create',{member_id:'default',name:'甲鞋',category:'shoes'});
 const memberB=(await call('member.create',{name:'配对成员乙'})).member_id as string;
 await call('clothing.create',{member_id:memberB,name:'乙锚点上衣',category:'top'});
 await call('clothing.create',{member_id:memberB,name:'乙下装',category:'bottom'});
 await call('clothing.create',{member_id:memberB,name:'乙鞋',category:'shoes'});
 let release:()=>void=()=>{};
 const gate=new Promise<void>(resolve=>{release=resolve;});
 await page.route('**/__wardrobe_dev_api',async route=>{
  const body=route.request().postData()??'';
  if(body.includes('pairing.list')&&body.includes(memberA)){await gate;await route.continue();return;}
  await route.continue();
 });
 await page.goto('/');
 await expect(page.locator('.collection .grid article').first()).toBeVisible();
 // The previous test persisted a different selected member: explicitly
 // return to member A so the gated pairing load below belongs to A.
 await page.getByText('当前衣橱').click();
 await page.getByRole('combobox',{name:/^当前成员/}).selectOption({label:'我'});
 // 切换是异步持久操作：等默认成员衣橱真实加载（种子≥24 件）即可；具体
 // 单品名可能被其他规格的正常管理操作改名，不做硬编码断言。
 await expect.poll(async()=>await page.locator('.collection .grid article').count(),{timeout:15000}).toBeGreaterThanOrEqual(24);
 await page.getByRole('navigation').getByRole('button',{name:'穿搭',exact:true}).click();
 await page.getByRole('button',{name:'单品配对',exact:true}).click();
 const studio=page.locator('#pairing');
 await expect(studio.getByRole('status').filter({hasText:'正在读取单品'})).toBeVisible();
 const memberBox=page.getByRole('combobox',{name:/^当前成员/});
 if(!await memberBox.isVisible())await page.getByText('当前衣橱').click();
 await memberBox.selectOption({label:'配对成员乙'});
 // The member switch resets the workspace to the wardrobe view: wait for the
 // fresh member's wardrobe to finish loading before navigating again, or the
 // workspace-tab click races the transition and lands on a replaced node.
 await expect(page.getByRole('heading',{name:'我的衣橱'})).toBeVisible();
 await expect.poll(async()=>await page.locator('.collection .grid article').count(),{timeout:15000}).toBe(3);
 // Navigate back into the pairing studio for the fresh member.
 await page.getByRole('navigation').getByRole('button',{name:'穿搭',exact:true}).click();
 await page.getByRole('button',{name:'单品配对',exact:true}).click();
 const fresh=page.locator('#pairing');
 await expect(fresh.getByLabel('选择锚点单品')).toBeEnabled();
 await fresh.getByLabel('选择锚点单品').selectOption({label:'乙锚点上衣'});
 await fresh.getByRole('button',{name:'为锚点搭配',exact:true}).click();
 await expect(fresh.locator('.pairing-proposal').first()).toBeVisible();
 release();
 await page.waitForTimeout(500);
 const options=fresh.getByLabel('选择锚点单品').locator('option');
 await expect(options.filter({hasText:'乙锚点上衣'})).toHaveCount(1);
 await expect(options.filter({hasText:'甲锚点上衣'})).toHaveCount(0);
 const history=fresh.locator('.pairing-history-item');
 const count=await history.count();
 expect(count).toBeGreaterThanOrEqual(1);
 for(let index=0;index<count;index++){
  await expect(history.nth(index)).toContainText('乙锚点上衣');
  await expect(history.nth(index)).not.toContainText('甲锚点上衣');
 }
});
