import {test,expect} from '@playwright/test';
// 单品管理验收（新 UI）：详情弹窗 → 编辑衣物（基本资料/照片管理/历史与管理），
// 库存状态经“筛选”弹窗切换。行为断言（编辑保留事实、归档、回收站、恢复、
// 各宽度可达、大额价格精确显示）保持不变；币种经购买信息内“币种”字段编辑。
const nav=(page:any,name:string)=>page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name,exact:true});
async function filterStatus(page:any,status:'衣橱'|'归档'|'回收站'){
 await page.getByRole('button',{name:'筛选',exact:true}).click();
 const dialog=page.getByRole('dialog',{name:'筛选衣物'});
 await dialog.getByRole('button',{name:status,exact:true}).click();
 await dialog.getByRole('button',{name:'应用筛选'}).click();
}
async function openEditor(page:any,article:any){
 await article.getByRole('button',{name:'查看详情'}).click();
 const editButton=page.getByRole('button',{name:/编辑衣物|恢复与历史/});
 await editButton.click();
 return page.getByRole('dialog');
}
test('details edit, archive, recycle and restore clothing without losing facts',async({page})=>{
 await page.goto('/');const seededCount=async(page:any)=>{const r=await(await page.request.post('/__wardrobe_dev_api',{data:{operation:'clothing.list',input:{member_id:'default',status:'active',limit:100}}})).json();return r.result.items.length;};await expect.poll(async()=>await page.getByRole('article').count(),{timeout:15000}).toBe(Math.min(await seededCount(page),24));
 let dialog=await openEditor(page,page.getByRole('article').first());
 await dialog.getByLabel('名称',{exact:true}).fill('回收站验收单品');
 await dialog.locator('summary').filter({hasText:'详细属性'}).click();
 await dialog.getByLabel('材质',{exact:true}).fill('棉，麻');
 await dialog.getByLabel('品牌',{exact:true}).fill('手工品牌');
 await dialog.getByRole('button',{name:'保存修改'}).click();
 await expect(dialog.getByRole('status')).toContainText('已更新');
 // 归档：历史与管理分区操作，随后在筛选中查看归档库。
 await dialog.getByRole('button',{name:'历史与管理',exact:true}).click();
 await dialog.getByRole('button',{name:'归档单品',exact:true}).click();
 await expect(page.getByRole('status')).toContainText('已更新');
 await page.getByRole('dialog').getByRole('button',{name:'关闭单品窗口'}).click();
 await filterStatus(page,'归档');
 const archived=page.getByRole('article').filter({hasText:'回收站验收单品'});
 await expect(archived).toHaveCount(1);
 dialog=await openEditor(page,archived);
 await dialog.locator('summary').filter({hasText:'详细属性'}).click();
 await expect(dialog.getByLabel('品牌',{exact:true})).toHaveValue('手工品牌');
 await dialog.getByRole('button',{name:'历史与管理',exact:true}).click();
 await dialog.getByRole('button',{name:'移入回收站',exact:true}).click();
 await dialog.getByRole('button',{name:'确认移入',exact:true}).click();
 await expect(page.getByRole('article').filter({hasText:'回收站验收单品'})).toHaveCount(0);
 await filterStatus(page,'回收站');
 const trashed=page.getByRole('article').filter({hasText:'回收站验收单品'});
 await expect(trashed).toHaveCount(1);
 dialog=await openEditor(page,trashed);
 // 回收站单品的编辑器默认停在基础分区并直接提供恢复。
 await dialog.getByRole('button',{name:'恢复单品'}).click();
 await expect(page.getByRole('status')).toContainText('已更新');
 await page.getByRole('dialog').getByRole('button',{name:'关闭单品窗口'}).click();
 await filterStatus(page,'衣橱');
 await expect(page.getByRole('article').filter({hasText:'回收站验收单品'})).toHaveCount(1);
 dialog=await openEditor(page,page.getByRole('article').filter({hasText:'回收站验收单品'}));
 await dialog.getByRole('button',{name:'历史与管理',exact:true}).click();
 await dialog.getByRole('button',{name:'取消归档'}).click();
 await expect(page.getByRole('status')).toContainText('已更新');
 await page.getByRole('dialog').getByRole('button',{name:'关闭单品窗口'}).click();
 // 清理：回到回收站不影响后续规格的默认视图。
 await page.getByRole('dialog').getByRole('button',{name:'关闭单品窗口'}).click();
});

test('management controls remain reachable at supported narrow and desktop widths',async({page})=>{
 for(const width of [320,375,414,768,1024,1280,1440]) {await page.setViewportSize({width,height:900});await page.goto('/');await expect(page.getByRole('article').first()).toBeVisible();expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBeTruthy();await page.getByRole('article').first().getByRole('button',{name:'查看详情'}).click();await expect(page.getByRole('button',{name:'移入回收站',exact:true})).toBeVisible();await page.screenshot({path:`/tmp/wardrobe-ticket-04-ui-${width}.png`,fullPage:true});await page.keyboard.press('Escape');await expect(page.getByRole('dialog')).not.toBeVisible();}
});
test('large minor-unit prices display and edit without floating-point rounding',async({page})=>{
 await page.goto('/');await expect(page.getByRole('article').first()).toBeVisible();
 await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();
 const create=page.getByRole('dialog');
 await create.getByLabel('名称',{exact:true}).fill('大额价格验收');
 await create.getByRole('button',{name:'添加到衣橱'}).click();
 await expect(create).not.toBeVisible();
 const item=page.getByRole('article').filter({hasText:'大额价格验收'});
 await expect(item).toHaveCount(1);
 const dialog=await openEditor(page,item);
 await dialog.locator('summary').filter({hasText:'购买信息'}).click();
 await dialog.getByLabel('币种',{exact:true}).fill('BHD');
 await dialog.getByLabel(/购入价格/).fill('9007199254740.991');
 await dialog.getByRole('button',{name:'保存修改'}).click();
 await expect(dialog.getByRole('status')).toContainText('已更新');
 await page.getByRole('dialog').getByRole('button',{name:'关闭单品窗口'}).click();
 dialog=await openEditor(page,item);
 await dialog.locator('summary').filter({hasText:'购买信息'}).click();
 await expect(dialog.getByLabel(/购入价格/)).toHaveValue('9007199254740.991');
 await expect(dialog.getByLabel('币种',{exact:true})).toHaveValue('BHD');
 await dialog.getByRole('button',{name:'历史与管理',exact:true}).click();
 await dialog.getByRole('button',{name:'移入回收站',exact:true}).click();
 await dialog.getByRole('button',{name:'确认移入',exact:true}).click();
 await expect(item).toHaveCount(0);
});
