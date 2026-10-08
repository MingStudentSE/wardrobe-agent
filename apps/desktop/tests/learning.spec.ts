import {test,expect} from '@playwright/test';
// 穿衣偏好（学习记忆）验收（新 UI）：设置 → 穿衣偏好；画像自动加载，
// 重算在“从穿着与反馈中学习的偏好”详情内。行为断言（显式偏好、来源、
// 修正、删除不复活、重置/完整重建、幂等键、任务查询与取消）保持不变。
const nav=(page:any,name:string)=>page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name,exact:true});
async function openLearning(page:any){
 await nav(page,'设置').click();
 await page.getByRole('region',{name:'设置目录'}).getByRole('button',{name:/穿衣偏好/}).click();
 return page.locator('#learning');
}
async function backToHub(page:any){await page.getByRole('button',{name:'← 返回设置'}).click();}
async function openRecommend(page:any){
 await nav(page,'穿搭').click();
 await page.getByRole('button',{name:'帮我搭配',exact:true}).click();
 return page.locator('#recommendations');
}
test('显式偏好可查看编辑删除，重置与完整重建明确且不写穿着',async({page})=>{
 await page.goto('/');await page.getByText('当前衣橱',{exact:true}).click();await page.getByText('管理成员档案',{exact:true}).click();const name=`学习验收-${Date.now()}`;await page.getByLabel('新成员名称').fill(name);await page.getByRole('button',{name:'创建成员',exact:true}).click();const choice=page.getByRole('combobox',{name:'当前成员',exact:true});await expect(choice.getByRole('option',{name,exact:true})).toBeAttached();const member=await choice.getByRole('option',{name,exact:true}).getAttribute('value');await expect(choice).toHaveValue(member!);
 try{
 const panel=await openLearning(page);
 await expect(panel.locator('.learning-explicit')).toBeVisible();
 await panel.getByRole('button',{name:'＋ 添加偏好',exact:true}).click();
 let d=page.getByRole('dialog',{name:'添加穿衣偏好',exact:true});
 await d.getByLabel('偏好维度').selectOption('color');await d.getByLabel('偏好值').fill('blue');await d.getByLabel('偏好权重').fill('4');await d.getByLabel('偏好说明').fill('明确喜欢蓝色');await d.getByRole('button',{name:'保存明确偏好',exact:true}).click();
 await expect(panel.getByRole('status')).toContainText('明确偏好已保存');
 const explicit=panel.locator('.learning-explicit');await expect(explicit).toContainText('blue');
 await explicit.getByRole('button',{name:'编辑',exact:true}).click();
 d=page.getByRole('dialog',{name:'编辑穿衣偏好',exact:true});
 await d.getByLabel('偏好权重').fill('-2');await d.getByRole('button',{name:'保存明确偏好',exact:true}).click();await expect(explicit).toContainText('-2');
 await page.reload();await openLearning(page);await expect(explicit).toContainText('-2');
 await explicit.getByRole('button',{name:'删除',exact:true}).click();await panel.getByLabel('确认删除此记忆').check();await panel.getByRole('button',{name:'确认删除记忆',exact:true}).click();await expect(explicit).not.toContainText('blue');
 await panel.locator('details.learning-inferred').locator('>summary').click();
 await panel.getByRole('button',{name:'按当前边界重算',exact:true}).click();await expect(panel.getByRole('status')).toContainText('重算已完成');
 await panel.locator('details').filter({hasText:'重置与完整重建'}).locator('summary').click();
 await panel.getByLabel('确认重置推断画像').check();await panel.getByRole('button',{name:'重置推断画像',exact:true}).click();await expect(panel.getByRole('status')).toContainText('重置已完成');
 for(const width of [320,375,414,768,1024,1280,1440]){await page.setViewportSize({width,height:1000});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBeTruthy();if(width===320||width===1280)await panel.screenshot({path:`../../docs/verification/ticket-18/learning-${width}.png`});}
 const response=await page.request.post('/__wardrobe_dev_api',{data:{operation:'wear.list',input:{member_id:member}}});expect((await response.json()).result.items).toEqual([]);
 }finally{const choice=page.getByRole('combobox',{name:'当前成员',exact:true});if(!(await choice.isVisible()))await page.getByText('当前衣橱',{exact:true}).click();await expect(choice).toBeEnabled();await choice.selectOption({label:'我'});}
});
test('真实事件画像可读来源、修正排序、删除不复活，显式完整重建保留事实',async({page})=>{
 await page.goto('/');await page.getByText('当前衣橱',{exact:true}).click();await page.getByText('管理成员档案',{exact:true}).click();const name=`画像验收-${Date.now()}`;await page.getByLabel('新成员名称').fill(name);await page.getByRole('button',{name:'创建成员',exact:true}).click();const choice=page.getByRole('combobox',{name:'当前成员',exact:true});await expect(choice.getByRole('option',{name,exact:true})).toBeAttached();const member=await choice.getByRole('option',{name,exact:true}).getAttribute('value');await expect(choice).toHaveValue(member!);
 const call=async(operation:string,input:object={})=>{const r=await page.request.post('/__wardrobe_dev_api',{data:{operation,input:{member_id:member,...input}}});const body=await r.json();if(body.error)throw new Error(body.error.message);return body.result;};
 try{
 const red=await call('clothing.create',{name:'红上衣',category:'top',color:['red']});const blue=await call('clothing.create',{name:'蓝上衣',category:'top',color:['blue']});for(const category of ['bottom','shoes'])await call('clothing.create',{name:category,category});
 const wear=await call('wear.record',{operation_key:'actualwearfixture',clothing_ids:[blue.id],local_date:'2020-01-01',timezone:'UTC'});const facts=await call('wear.list');
 const panel=await openLearning(page);
 const inferred=panel.locator('details.learning-inferred');
 await inferred.locator('>summary').click();
 await panel.getByRole('button',{name:'按当前边界重算',exact:true}).click();
 await expect(inferred).toContainText('颜色 · blue');
 const row=inferred.getByRole('listitem').filter({hasText:'颜色 · blue'});await expect(row).toContainText('权重 1');await row.locator('summary').filter({hasText:'来源事件 ID'}).click();await expect(row).toContainText(wear.id);
 for(const width of [320,1280]){await page.setViewportSize({width,height:1000});await panel.screenshot({path:`../../docs/verification/ticket-18/learning-sources-${width}.png`});}await page.setViewportSize({width:1280,height:1000});
 await row.getByRole('button',{name:'修正为明确偏好',exact:true}).click();
 // 修正推断时 editing 为空：对话框沿用“添加穿衣偏好”，但表单预填推断特征。
 const d=page.getByRole('dialog',{name:'添加穿衣偏好',exact:true});
 await expect(d.getByLabel('偏好值')).toHaveValue('blue');
 await d.getByLabel('偏好权重').fill('5');await d.getByRole('button',{name:'保存明确偏好',exact:true}).click();await expect(panel.locator('.learning-explicit')).toContainText('权重 5');
 const preferred=await call('recommendation.generate',{operation_key:'preferblue',timezone:'UTC',limit:1});expect(preferred.proposals[0].items.some((v:any)=>v.clothing_id===blue.id)).toBeTruthy();
 await row.getByRole('button',{name:'删除',exact:true}).click();await panel.getByLabel('确认删除此记忆').check();await panel.getByRole('button',{name:'确认删除记忆',exact:true}).click();
 await panel.getByRole('button',{name:'按当前边界重算',exact:true}).click();await expect(inferred).not.toContainText('颜色 · blue');await expect(panel.locator('.learning-explicit')).not.toContainText('颜色 · blue');
 await panel.locator('details').filter({hasText:'重置与完整重建'}).locator('summary').click();
 await expect(panel.getByRole('button',{name:'完整重建画像',exact:true})).toBeDisabled();await panel.getByLabel('确认重新纳入此前全部事实').check();await panel.getByRole('button',{name:'完整重建画像',exact:true}).click();await expect(inferred).toContainText('权重 1');expect(await call('wear.list')).toEqual(facts);expect((await call('clothing.get',{id:red.id})).color).toEqual(['red']);
 await panel.locator('details').filter({hasText:'偏好修改历史'}).locator('summary').click();
 await panel.getByRole('button',{name:'查看修改历史',exact:true}).click();await expect(panel).toContainText('learning.delete');
 }finally{const choice=page.getByRole('combobox',{name:'当前成员',exact:true});if(!(await choice.isVisible()))await page.getByText('当前衣橱',{exact:true}).click();await expect(choice).toBeEnabled();await choice.selectOption({label:'我'});}
});
test('确认记忆修改后的再次推荐使用新意图，未知响应重试保持原键',async({page})=>{
 await page.goto('/');await page.getByText('当前衣橱',{exact:true}).click();await page.getByText('管理成员档案',{exact:true}).click();const name=`偏好推荐-${Date.now()}`;await page.getByLabel('新成员名称').fill(name);await page.getByRole('button',{name:'创建成员',exact:true}).click();const choice=page.getByRole('combobox',{name:'当前成员',exact:true});await expect(choice.getByRole('option',{name,exact:true})).toBeAttached();const member=await choice.getByRole('option',{name,exact:true}).getAttribute('value');await expect(choice).toHaveValue(member!);
 const call=async(operation:string,input:object={})=>{const body=await(await page.request.post('/__wardrobe_dev_api',{data:{operation,input:{member_id:member,...input}}})).json();if(body.error)throw new Error(body.error.message);return body.result;};
 try{
 for(const [name,category,color] of [['先创建红上衣','top','red'],['后创建蓝上衣','top','blue'],['偏好裤','bottom','black'],['偏好鞋','shoes','black']])await call('clothing.create',{name,category,color:[color]});
 const rec=await openRecommend(page);await rec.getByRole('button',{name:'刷新推荐库存',exact:true}).click();await rec.getByRole('button',{name:'为我推荐搭配'}).click();await expect(rec.locator('.recommendation-proposal').first()).toContainText('先创建红上衣');
 const panel=await openLearning(page);
 await panel.getByRole('button',{name:'＋ 添加偏好',exact:true}).click();
 const d=page.getByRole('dialog',{name:'添加穿衣偏好',exact:true});
 await d.getByLabel('偏好值').fill('blue');await d.getByLabel('偏好权重').fill('5');
 const operationKeys:string[]=[];await page.route('**/__wardrobe_dev_api',async route=>{const data=route.request().postDataJSON();if(data.operation==='learning.set'){operationKeys.push(data.input.operation_key);const response=await route.fetch();if(operationKeys.length===1){await route.fulfill({status:503,contentType:'application/json',body:JSON.stringify({error:{message:'偏好已记录，响应丢失'}})});return;}await route.fulfill({response});return;}await route.continue();});
 await d.getByRole('button',{name:'保存明确偏好',exact:true}).click();await expect(panel.getByRole('alert')).toContainText('响应丢失');
 const d2=page.getByRole('dialog',{name:'添加穿衣偏好',exact:true});
 await d2.getByRole('button',{name:'保存明确偏好',exact:true}).click();await expect(panel.getByRole('status')).toContainText('明确偏好已保存');expect(operationKeys).toHaveLength(2);expect(operationKeys[0]).toBe(operationKeys[1]);expect((await call('learning.get')).explicit).toHaveLength(1);
 const rec2=await openRecommend(page);await rec2.getByRole('button',{name:'为我推荐搭配'}).click();await expect(rec2.locator('.recommendation-proposal').first()).toContainText('后创建蓝上衣');expect((await call('wear.list')).items).toEqual([]);
 }finally{const choice=page.getByRole('combobox',{name:'当前成员',exact:true});if(!(await choice.isVisible()))await page.getByText('当前衣橱',{exact:true}).click();await expect(choice).toBeEnabled();await choice.selectOption({label:'我'});}
});
test('学习重算立即显示持久任务，重开可查询并明确取消而不发布画像',async({page})=>{
 await page.goto('/');await page.getByText('当前衣橱',{exact:true}).click();await page.getByText('管理成员档案',{exact:true}).click();const name=`异步学习-${Date.now()}`;await page.getByLabel('新成员名称').fill(name);await page.getByRole('button',{name:'创建成员',exact:true}).click();const choice=page.getByRole('combobox',{name:'当前成员',exact:true});await expect(choice.getByRole('option',{name,exact:true})).toBeAttached();const member=await choice.getByRole('option',{name,exact:true}).getAttribute('value');await expect(choice).toHaveValue(member!);
 let release!:()=>void;const gate=new Promise<void>(resolve=>release=resolve);let workers=0;
 await page.route('**/__wardrobe_dev_api',async route=>{if(route.request().postDataJSON().operation==='learning.worker.run'){workers++;await gate;}await route.continue();});
 try{
 const panel=await openLearning(page);
 const inferred=panel.locator('.learning-inferred');
 await inferred.locator('summary').click();
 const receipt=page.waitForResponse(r=>r.request().postDataJSON()?.operation==='learning.recompute');await panel.getByRole('button',{name:'按当前边界重算',exact:true}).click();const body=await(await receipt).json();const id=body.result.job_id;expect(id).toBeTruthy();expect(body.result.state).toBe('queued');await expect(panel.getByRole('status')).toContainText(id);await expect.poll(()=>workers).toBe(1);
 await page.reload();await openLearning(page);
 await panel.locator('details').filter({hasText:'学习任务'}).locator('summary').click();
 await panel.getByRole('button',{name:'查询或继续重算任务',exact:true}).click();await expect(panel.getByRole('region',{name:'学习重算任务'})).toContainText(id);await panel.getByRole('button',{name:'取消重算',exact:true}).click();await expect(panel.getByRole('status')).toContainText('已取消');release();
 const call=async(operation:string,input:object={})=>(await(await page.request.post('/__wardrobe_dev_api',{data:{operation,input:{member_id:member,...input}}})).json()).result;
 expect((await call('jobs.get',{id})).state).toBe('cancelled');expect((await call('learning.get')).inferred).toEqual([]);expect((await call('wear.list')).items).toEqual([]);
 }finally{release();const choice=page.getByRole('combobox',{name:'当前成员',exact:true});if(!(await choice.isVisible()))await page.getByText('当前衣橱',{exact:true}).click();await expect(choice).toBeEnabled();await choice.selectOption({label:'我'});}
});
