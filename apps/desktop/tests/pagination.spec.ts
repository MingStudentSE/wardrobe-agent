import {test,expect} from '@playwright/test';
test('repeated load-more clicks show every real item once',async({page})=>{
 await page.route('**/__wardrobe_dev_api',async route=>{
  if(route.request().postDataJSON().input?.cursor)await new Promise(resolve=>setTimeout(resolve,250));
  await route.continue();
 });
 await page.goto('/');
 await expect(page.getByRole('article')).toHaveCount(24);
 await page.getByRole('button',{name:'加载更多'}).evaluate((button:HTMLButtonElement)=>{button.click();button.click();});
 await expect(page.getByRole('button',{name:'加载更多'})).toHaveCount(0);
 await expect(page.getByRole('article')).toHaveCount(25);
});
test('fresh save invalidates an older pending page response',async({page})=>{
 await page.route('**/__wardrobe_dev_api',async route=>{
  if(route.request().postDataJSON().input?.cursor)await new Promise(resolve=>setTimeout(resolve,1200));
  await route.continue();
 });
 await page.goto('/');await expect(page.getByRole('article')).toHaveCount(24);
 const pending=page.waitForResponse(response=>response.url().includes('__wardrobe_dev_api')&&Boolean(response.request().postDataJSON().input?.cursor));
 await page.getByRole('button',{name:'加载更多'}).click();
 await expect(page.getByRole('button',{name:'正在加载…'})).toBeDisabled();
 await page.getByLabel('名称',{exact:true}).fill('刷新保存');await page.getByRole('button',{name:'保存单品'}).click();
 await expect(page.getByRole('status')).toContainText('已保存到本地衣橱');await pending;
 await expect(page.getByRole('article')).toHaveCount(24);
});
test('manual price input preserves a three-decimal currency exactly',async({page})=>{
 await page.goto('/');await expect(page.getByRole('article')).toHaveCount(24);
 await page.getByLabel('名称',{exact:true}).fill('三位小数价格');
 await page.getByLabel('币种',{exact:true}).fill('BHD');
 await page.getByLabel('购入价格',{exact:true}).fill('1.234');
 await page.getByRole('button',{name:'保存单品'}).click();
 await expect(page.getByRole('status')).toContainText('已保存到本地衣橱');
 await page.getByRole('button',{name:'加载更多'}).click();
 const item=page.getByRole('article').filter({has:page.getByRole('heading',{name:'三位小数价格'})});
 await expect(item).toContainText('1.234 BHD');
});
test('load-more stays unavailable during post-save replacement query',async({page})=>{
 let saved=false;
 await page.route('**/__wardrobe_dev_api',async route=>{
  const request=route.request().postDataJSON();
  if(request.operation==='clothing.create')saved=true;
  if(saved&&request.operation==='clothing.list'&&!request.input?.cursor)await new Promise(resolve=>setTimeout(resolve,1000));
  await route.continue();
 });
 await page.goto('/');await expect(page.getByRole('article')).toHaveCount(24);
 await page.getByLabel('名称',{exact:true}).fill('延迟刷新录入');
 await page.getByRole('button',{name:'保存单品'}).click();
 await expect(page.getByRole('button',{name:'正在保存…'})).toBeDisabled();
 await expect(page.getByRole('button',{name:'加载更多'})).toBeDisabled();
 await expect(page.getByRole('status')).toContainText('已保存到本地衣橱');
 await expect(page.getByRole('button',{name:'加载更多'})).toBeEnabled();
});
