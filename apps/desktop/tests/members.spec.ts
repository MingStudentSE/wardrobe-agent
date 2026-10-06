import {test,expect} from '@playwright/test';
test('create edit switch and reload an independent member wardrobe',async({page})=>{
 await page.goto('/');
 await page.getByText('管理成员档案',{exact:true}).click();
 await page.getByLabel('新成员名称').fill('妹妹');
 await page.getByRole('button',{name:'创建成员',exact:true}).click();
 await expect(page.getByLabel('当前成员')).toHaveValue(/.+/);
 await page.getByLabel('成员名称',{exact:true}).fill('家人');
 await page.getByRole('button',{name:'保存成员名称'}).click();
 await expect(page.getByLabel('当前成员').getByRole('option',{name:'家人'})).toBeAttached();
 await expect(page.getByRole('article')).toHaveCount(0);
 await page.getByLabel('名称',{exact:true}).fill('独立成员的衣服');
 await page.getByRole('button',{name:'保存单品'}).click();
 await expect(page.getByRole('article')).toHaveCount(1);
 await page.reload();await expect(page.getByRole('article')).toHaveCount(1);
 await page.getByLabel('当前成员').selectOption({label:'我'});
 await expect(page.getByRole('article')).toHaveCount(24);
});
test('member controls stay usable at every required viewport',async({page})=>{
 await page.goto('/');await expect(page.getByLabel('当前成员')).toBeEnabled();
 for(const width of [320,375,414,768,1024,1280,1440]){
  await page.setViewportSize({width,height:900});
  await expect(page.getByLabel('当前成员')).toBeVisible();
  const overflow=await page.evaluate(()=>document.documentElement.scrollWidth>window.innerWidth);
  expect(overflow,`horizontal overflow at ${width}`).toBe(false);
 }
 await page.getByText('管理成员档案',{exact:true}).focus();await page.keyboard.press('Enter');await expect(page.getByLabel('新成员名称')).toBeVisible();
});
test('saving cannot race a pending member switch',async({page})=>{
 await page.goto('/');await expect(page.getByLabel('当前成员')).toBeEnabled();
 await page.route('**/__wardrobe_dev_api',async route=>{if(route.request().postDataJSON().operation==='member.select')await new Promise(resolve=>setTimeout(resolve,700));await route.continue();});
 await page.getByLabel('当前成员').selectOption({label:'家人'});
 await expect(page.getByRole('button',{name:'保存单品'})).toBeDisabled();
 await expect(page.getByRole('article')).toHaveCount(1);
 await page.getByLabel('当前成员').selectOption({label:'我'});
 await expect(page.getByRole('article')).toHaveCount(24);
});
