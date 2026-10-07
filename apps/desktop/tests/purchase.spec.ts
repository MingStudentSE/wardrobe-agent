import {test,expect} from '@playwright/test';
// 购买适配分析（issue #24）通过真实公开应用验收：候选草稿存放在候选域、
// 不进入衣物库存；纠错显式发生；三项分析（搭配潜力/重复风险/缺口）引用
// 真实库存并以版本化历史保留。种子数据经共享 dev API（同一 Application API）。
test.afterEach(async({page})=>{
 // 成员选择是持久化状态：切回默认成员（与 imports/feedback/learning 等规格
 // 一致），避免污染共享 dev server 上后续规格（如 stylist 以默认成员 id
 // 供工具帧引用）。select 值等于「我」即证明 member.select 已持久化。
 const choice=page.getByRole('combobox',{name:/^当前成员/});
 if(!(await choice.isVisible()))await page.getByText('当前衣橱').click();
 await expect(choice).toBeEnabled();
 await choice.selectOption({label:'我'});
 await expect(choice).toHaveValue(await choice.getByRole('option',{name:'我',exact:true}).getAttribute('value')??'');
});
test('candidate draft stays out of inventory and analysis cites real stock',async({page})=>{
 const call=async(operation:string,input:object)=>{const r=await (await page.request.post('/__wardrobe_dev_api',{data:{operation,input}})).json();if(r.error)throw Error(r.error.message);return r.result;};
 const member=(await call('member.create',{name:'候选分析成员'})).member_id as string;
 await call('clothing.create',{member_id:member,name:'候选库存白衬衫',category:'top',color:['white']});
 await call('clothing.create',{member_id:member,name:'候选库存黑鞋',category:'shoes',color:['black']});
 await page.goto('/');
 await expect(page.locator('.collection .grid article').first()).toBeVisible();
 await page.getByText('当前衣橱').click();
 await page.getByRole('combobox',{name:/^当前成员/}).selectOption({label:'候选分析成员'});
 // 成员切换是异步持久操作：等概览计数反映新成员后再导航，避免视图被切换回写。
 await expect(page.locator('.wardrobe-counts')).toContainText('2 件单品');
 await page.getByRole('navigation').getByRole('button',{name:'穿搭',exact:true}).click();
 await page.getByRole('button',{name:'购买分析',exact:true}).click();
 const studio=page.locator('#purchase');
 await expect(studio).toBeVisible();
 // 候选草稿创建（价格/尺码留空 = 未知）。
 await studio.getByLabel('商品名称').fill('候选蓝牛仔裤');
 await studio.getByLabel('类别').selectOption({label:'下装'});
 await studio.locator('.purchase-create .purchase-colors label',{hasText:'蓝'}).locator('input').check();
 await studio.getByRole('button',{name:'保存候选草稿'}).click();
 const card=studio.locator('.purchase-candidate', {hasText:'候选蓝牛仔裤'});
 await expect(card).toContainText('尺码未知');
 await expect(card).toContainText('价格未知');
 // 三项分析：真实库存组合 + 填补空洞 + 不确定性如实。
 await card.getByRole('button',{name:'分析此候选'}).click();
 const result=card.locator('.purchase-analysis-result');
 await expect(result).toBeVisible();
 await expect(result).toContainText('与真实库存可组成 1 种组合');
 await expect(result).toContainText('〔候选〕候选蓝牛仔裤');
 await expect(result).toContainText('候选库存白衬衫');
 await expect(result).toContainText('候选库存黑鞋');
 await expect(result).toContainText('候选填补空缺类别：下装');
 await expect(result).toContainText('候选本身不计入库存');
 // 候选与已有严格区分：库存列表里没有候选；候选不能被穿着（结构上不属于衣物域）。
 await page.getByRole('navigation').getByRole('button',{name:'衣橱',exact:true}).click();
 await expect(page.locator('.collection .grid article', {hasText:'候选蓝牛仔裤'})).toHaveCount(0);
 await expect(page.locator('.collection .grid article', {hasText:'候选库存白衬衫'})).toHaveCount(1);
 // 版本化历史：再次分析后可选择 v1/v2。
 await page.getByRole('navigation').getByRole('button',{name:'穿搭',exact:true}).click();
 await page.getByRole('button',{name:'购买分析',exact:true}).click();
 await card.getByRole('button',{name:'分析此候选'}).click();
 // 第二次分析是异步运行：等版本选择器出现 v1/v2 两个历史版本。
 await expect(card.locator('label',{hasText:'查看历史版本'}).locator('option')).toHaveCount(2,{timeout:15000});
});
test('manual correction is explicit and recognition suggestions stay unverified',async({page})=>{
 const call=async(operation:string,input:object)=>{const r=await (await page.request.post('/__wardrobe_dev_api',{data:{operation,input}})).json();if(r.error)throw Error(r.error.message);return r.result;};
 const member=(await call('member.create',{name:'候选纠错成员'})).member_id as string;
 await page.goto('/');
 await expect(page.locator('.collection .grid article').first()).toBeVisible();
 await page.getByText('当前衣橱').click();
 await page.getByRole('combobox',{name:/^当前成员/}).selectOption({label:'候选纠错成员'});
 await expect(page.locator('.wardrobe-counts')).toContainText('0 件单品 · 0 套穿搭');
 await page.getByRole('navigation').getByRole('button',{name:'穿搭',exact:true}).click();
 await page.getByRole('button',{name:'购买分析',exact:true}).click();
 const studio=page.locator('#purchase');
 await studio.getByLabel('商品名称').fill('候选外套');
 await studio.getByLabel('类别').selectOption({label:'外套'});
 await studio.getByRole('button',{name:'保存候选草稿'}).click();
 const card=studio.locator('.purchase-candidate', {hasText:'候选外套'});
 await expect(card).toBeVisible();
 // 无视觉模型时识别区提示先配置；人工纠错独立可用。
 await expect(card).toContainText('请先在模型设置添加视觉模型');
 await card.locator('.purchase-correct summary').click();
 await card.getByLabel('描述更正').fill('人工确认的卡其外套');
 await card.locator('.purchase-correct .purchase-colors label',{hasText:'米色'}).locator('input').check();
 await card.getByLabel('我已核对以上更正内容').check();
 await card.getByRole('button',{name:'保存纠错'}).click();
 await expect(card).toContainText('人工确认的卡其外套');
 await expect(card).toContainText('米色');
 // 弃置后从草稿列表消失。
 await card.getByRole('button',{name:'弃置候选'}).click();
 await expect(studio.locator('.purchase-candidate', {hasText:'候选外套'})).toHaveCount(0);
 // 候选分析入口在 320–1440 宽度内无横向溢出。
 for(const width of [320,375,768,1440]){
  await page.setViewportSize({width,height:1000});
  await expect(studio.getByRole('button',{name:'保存候选草稿'})).toBeVisible();
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`购买分析 ${width}`).toBeTruthy();
 }
});
