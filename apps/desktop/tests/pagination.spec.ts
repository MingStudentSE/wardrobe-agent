import {test,expect} from '@playwright/test';
test('repeated load-more clicks show every real item once',async({page})=>{
 await page.route('**/__wardrobe_dev_api',async route=>{
  if(route.request().postDataJSON().input?.cursor)await new Promise(resolve=>setTimeout(resolve,250));
  await route.continue();
 });
 await page.goto('/');
 await expect(page.locator('.collection .grid article').first()).toBeVisible();
 const call=async(operation:string,input:object)=>{const b=await(await page.request.post('/__wardrobe_dev_api',{data:{operation,input}})).json();if(b.error)throw new Error(b.error.message);return b.result;};
 const total=(await call('clothing.list',{member_id:'default',status:'active',limit:100})).items.length;
 // 连续点击直到加载更多消失：重复点击不产生重复条目，最终覆盖全部真实库存。
 for(let round=0;round<6&&(await page.getByRole('button',{name:'加载更多'}).count())>0;round++){
  await page.getByRole('button',{name:'加载更多'}).click();
  await page.waitForResponse(r=>r.url().includes('__wardrobe_dev_api')&&Boolean(r.request().postDataJSON().input?.cursor));
 }
 await expect(page.getByRole('button',{name:'加载更多'})).toHaveCount(0);
 const names=await page.locator('.collection .grid article h3').allTextContents();
 expect(new Set(names).size).toBe(names.length);
 expect(names.length).toBe(total);
});
test('fresh save invalidates an older pending page response',async({page})=>{
 await page.route('**/__wardrobe_dev_api',async route=>{
  if(route.request().postDataJSON().input?.cursor)await new Promise(resolve=>setTimeout(resolve,1200));
  await route.continue();
 });
 await page.goto('/');await expect(page.getByRole('article').first()).toBeVisible();
 const pending=page.waitForResponse(response=>response.url().includes('__wardrobe_dev_api')&&Boolean(response.request().postDataJSON().input?.cursor));
 await page.getByRole('button',{name:'加载更多'}).click();
 await expect(page.getByRole('button',{name:'正在加载…'})).toBeDisabled();
 const shown=await page.getByRole('article').count();
 // 经新增衣物弹窗真实保存：保存触发替换查询，过期分页响应被丢弃。
 await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();
 const dialog=page.getByRole('dialog');
 await dialog.getByLabel('名称',{exact:true}).fill('刷新保存');
 await dialog.getByRole('button',{name:'添加到衣橱',exact:true}).click();
 await expect(dialog).not.toBeVisible();
 await pending;
 // 迟到的分页响应被丢弃，列表保持替换后的真实第一页。
 await expect(page.getByRole('article')).toHaveCount(shown);
});
test('manual price input preserves a three-decimal currency exactly',async({page})=>{
 await page.goto('/');await expect(page.getByRole('article').first()).toBeVisible();
 await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();
 const create=page.getByRole('dialog');
 await create.getByLabel('名称',{exact:true}).fill('三位小数价格');
 await create.getByRole('button',{name:'添加到衣橱',exact:true}).click();
 await expect(create).not.toBeVisible();
 // 新单品按录入顺序在第二页：加载更多后可见。
 await page.getByRole('button',{name:'加载更多'}).click();
 const item=page.getByRole('article').filter({has:page.getByRole('heading',{name:'三位小数价格'})});
 await expect(item).toHaveCount(1);
 await item.getByRole('button',{name:'查看详情'}).click();
 await page.getByRole('button',{name:'编辑衣物',exact:true}).click();
 const dialog=page.getByRole('dialog');
 await dialog.locator('summary').filter({hasText:'购买信息'}).click();
 await dialog.getByLabel('币种',{exact:true}).fill('BHD');
 await dialog.getByLabel(/购入价格/).fill('1.234');
 await dialog.getByRole('button',{name:'保存修改'}).click();
 await expect(dialog.getByRole('status')).toContainText('已更新');
 await page.getByRole('dialog').getByRole('button',{name:'关闭单品窗口'}).click();
 // 保存后列表替换回第一页：搜索定位后再重读，价格不因浮点失真。
 await page.getByLabel('搜索衣物',{exact:true}).fill('三位小数价格');
 await page.getByRole('button',{name:'搜索',exact:true}).click();
 await item.getByRole('button',{name:'查看详情'}).click();
 await page.getByRole('button',{name:'编辑衣物',exact:true}).click();
 await dialog.locator('summary').filter({hasText:'购买信息'}).click();
 await expect(dialog.getByLabel(/购入价格/)).toHaveValue('1.234');
 await expect(dialog.getByLabel('币种',{exact:true})).toHaveValue('BHD');
 await page.getByRole('dialog').getByRole('button',{name:'关闭单品窗口'}).click();
});
test('load-more stays unavailable during post-save replacement query',async({page})=>{
 let saved=false;
 await page.route('**/__wardrobe_dev_api',async route=>{
  const request=route.request().postDataJSON();
  if(request.operation==='clothing.create')saved=true;
  if(saved&&request.operation==='clothing.list'&&!request.input?.cursor)await new Promise(resolve=>setTimeout(resolve,1000));
  await route.continue();
 });
 await page.goto('/');await expect(page.getByRole('article').first()).toBeVisible();
 await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();
 const createDialog=page.getByRole('dialog');
 await createDialog.getByLabel('名称',{exact:true}).fill('延迟刷新录入');
 await createDialog.getByRole('button',{name:'添加到衣橱',exact:true}).click();
 // 保存期间替换查询进行：加载更多保持不可用，完成后恢复。
 await expect(page.getByRole('button',{name:'加载更多'})).toBeDisabled();
 await expect(page.getByRole('button',{name:'加载更多'})).toBeEnabled();
});
