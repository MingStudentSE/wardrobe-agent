import {test,expect} from '@playwright/test';
import {restoreDefaultMember} from './support/member-hygiene';
// 成员档案验收（新 UI）：成员切换器在侧栏“当前衣橱”内，档案管理在
// “管理成员档案”详情。行为断言（独立衣橱、跨重启、命名、切换锁与竞态）保持不变。
// 本规格会切换当前成员；结束后必须恢复默认成员，否则共享 dev server 上
// 后续规格会读到错误衣橱（竞态测试的最后一次切换可能随页面关闭而未落库）。
const nav=(page:any,name:string)=>page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name,exact:true});
async function createItemViaDialog(page:any,name:string){
 await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();
 const dialog=page.getByRole('dialog');
 await dialog.getByLabel('名称',{exact:true}).fill(name);
 await dialog.getByRole('button',{name:'添加到衣橱',exact:true}).click();
 await expect(dialog).not.toBeVisible();
}
test('create edit switch and reload an independent member wardrobe',async({page})=>{
 await page.goto('/');
 await page.getByText('当前衣橱',{exact:true}).click();
 await page.getByText('管理成员档案',{exact:true}).click();
 await page.getByLabel('新成员名称').fill('妹妹');
 await page.getByRole('button',{name:'创建成员',exact:true}).click();
 await expect(page.getByRole('combobox',{name:/^当前成员/})).toHaveValue(/.+/);
 await page.getByLabel('成员名称',{exact:true}).fill('家人');
 await page.getByRole('button',{name:'保存成员名称'}).click();
 await expect(page.getByRole('combobox',{name:/^当前成员/}).getByRole('option',{name:'家人'})).toBeAttached();
 await expect(page.getByRole('article')).toHaveCount(0);
 await createItemViaDialog(page,'独立成员的衣服');
 await expect(page.getByRole('article')).toHaveCount(1);
 await page.reload();await expect(page.getByRole('article')).toHaveCount(1);
 await page.getByText('当前衣橱',{exact:true}).click();
 await page.getByRole('combobox',{name:/^当前成员/}).selectOption({label:'我'});
 // 切换是异步持久操作：等默认成员衣橱真实加载完成（种子≥24 件）再断言。
 await expect.poll(async()=>await page.locator('.collection .grid article').count(),{timeout:15000}).toBeGreaterThanOrEqual(24);
});
test('member controls stay usable at every required viewport',async({page})=>{
 await page.goto('/');
 await page.getByText('当前衣橱',{exact:true}).click();
 await expect(page.getByRole('combobox',{name:/^当前成员/})).toBeEnabled();
 for(const width of [320,375,414,768,1024,1280,1440]){
  await page.setViewportSize({width,height:900});
  await expect(page.getByRole('combobox',{name:/^当前成员/})).toBeVisible();
  const overflow=await page.evaluate(()=>document.documentElement.scrollWidth>window.innerWidth);
  expect(overflow,`horizontal overflow at ${width}`).toBe(false);
 }
 await page.getByText('管理成员档案',{exact:true}).focus();await page.keyboard.press('Enter');await expect(page.getByLabel('新成员名称')).toBeVisible();
});
test('saving cannot race a pending member switch',async({page})=>{
 await page.goto('/');
 await page.getByText('当前衣橱',{exact:true}).click();
 await expect(page.getByRole('combobox',{name:/^当前成员/})).toBeEnabled();
 await page.route('**/__wardrobe_dev_api',async route=>{if(route.request().postDataJSON().operation==='member.select')await new Promise(resolve=>setTimeout(resolve,700));await route.continue();});
 await page.getByRole('combobox',{name:/^当前成员/}).selectOption({label:'家人'});
 await expect(page.getByRole('button',{name:'＋ 新增衣物',exact:true})).toBeDisabled();
 await expect(page.getByRole('article')).toHaveCount(1);
 await page.getByRole('combobox',{name:/^当前成员/}).selectOption({label:'我'});
 // 切换是异步持久操作：竞态路由会延迟 member.select，等默认成员衣橱真实
 // 加载完成再结束，避免页面关闭时切换尚未落库、污染后续规格。
 await expect.poll(async()=>await page.locator('.collection .grid article').count(),{timeout:15000}).toBeGreaterThanOrEqual(24);
 await expect(page.getByRole('article').first()).toBeVisible();
});
test.afterEach(async({request})=>{await restoreDefaultMember(request);});
