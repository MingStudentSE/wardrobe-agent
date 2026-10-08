import {test,expect} from '@playwright/test';
test('single composer saves an outfit, library edits and recovers it, recommendations save without wear',async({page})=>{
 await page.goto('/');await expect(page.locator('.collection .grid article').first()).toBeVisible();
 const call=async(operation:string,input:object={})=>{const r=await (await page.request.post('/__wardrobe_dev_api',{data:{operation,input:{member_id:'default',...input}}})).json();if(r.error)throw Error(r.error.message);return r.result;};
 await page.getByRole('navigation').getByRole('button',{name:'穿搭',exact:true}).click();
 await expect(page.getByRole('button',{name:'新建搭配',exact:true})).toHaveCount(0);
 await page.getByRole('button',{name:'去衣橱组搭配'}).click();
 await page.locator('.collection .grid article').first().getByRole('button',{name:'加入搭配'}).click();
 const composer=page.getByRole('complementary',{name:'搭配预览'});
 await composer.getByLabel('搭配名称').fill('穿搭工作室验收');await composer.getByRole('button',{name:'保存这套穿搭'}).click();await expect(composer.getByRole('status')).toContainText('搭配已保存');
 await page.getByRole('navigation').getByRole('button',{name:'穿搭',exact:true}).click();
 await page.getByRole('button',{name:'编辑 穿搭工作室验收',exact:true}).click();
 const dialog=page.getByRole('dialog',{name:'调整穿搭'});const stock=await call('clothing.list',{member_id:'default',status:'active',limit:100});await expect(dialog.locator('.outfit-garment-picker button')).toHaveCount(stock.items.length);
 await dialog.locator('.outfit-garment-picker button:not(:disabled)').first().click();await dialog.getByLabel('搭配名称',{exact:true}).fill('已调整的穿搭');await dialog.getByRole('button',{name:'保存搭配',exact:true}).click();await expect(dialog.getByRole('status')).toContainText('搭配已保存');
 await dialog.getByRole('button',{name:'移入搭配回收站'}).click();await dialog.getByRole('button',{name:'确认移入回收站'}).click();await dialog.getByRole('button',{name:'关闭单品窗口'}).click();
 await page.getByRole('button',{name:'回收站',exact:true}).click();await page.getByRole('button',{name:'编辑 已调整的穿搭',exact:true}).click();await dialog.getByRole('button',{name:'恢复搭配'}).click();await dialog.getByRole('button',{name:'关闭单品窗口'}).click();await page.getByRole('button',{name:'全部穿搭',exact:true}).click();await expect(page.getByRole('button',{name:'编辑 已调整的穿搭',exact:true})).toBeVisible();
 await page.getByRole('button',{name:'帮我搭配',exact:true}).click();const rec=page.locator('#recommendations');await expect(rec.getByLabel('推荐文本模型')).not.toBeVisible();await rec.getByRole('button',{name:'AI 推荐',exact:true}).click();await expect(rec.getByRole('button',{name:'AI 帮我搭配'})).toBeDisabled();await expect(rec.getByLabel('使用模型')).toBeVisible();await rec.getByRole('button',{name:'离线搭配',exact:true}).click();
 // 模式切换触发重渲染：等 aria-pressed 提交完成，再点生成，避免点击落在被替换的节点上触发原生表单提交。
 await expect(rec.getByRole('button',{name:'离线搭配',exact:true})).toHaveAttribute('aria-pressed','true');
 const wears=(await call('wear.list')).items;
 await rec.getByRole('button',{name:'为我推荐搭配'}).click();await expect(rec.locator('.recommendation-proposal').first()).toBeVisible();const proposal=rec.locator('.recommendation-proposal').first();await proposal.getByLabel('搭配保存名称').fill('推荐组合验收');await proposal.getByRole('button',{name:'保存为搭配',exact:true}).click();await expect(rec.getByRole('status').filter({hasText:'已保存到穿搭库'})).toBeVisible();expect((await call('wear.list')).items).toEqual(wears);await page.getByRole('button',{name:'穿搭库',exact:true}).click();await expect(page.getByRole('button',{name:'编辑 推荐组合验收',exact:true})).toBeVisible();await page.getByRole('button',{name:'帮我搭配',exact:true}).click();
 for(const width of [375,768,1440]){await page.setViewportSize({width,height:1000});await expect.poll(()=>page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);}
});
// Pending mutations must visibly hold the editor dialog open (disabled
// close, backdrop and cancel) and release it exactly when the public
// outfit.delete response completes, so following actions are never
// intercepted by a silently stuck modal.
test('editor close stays disabled during pending delete and closes after completion',async({page})=>{
 const call=async(operation:string,input:object)=>{const r=await (await page.request.post('/__wardrobe_dev_api',{data:{operation,input}})).json();if(r.error)throw Error(r.error.message);return r.result;};
 const top=await call('clothing.create',{member_id:'default',name:'延迟关闭上衣',category:'top'});
 const bottom=await call('clothing.create',{member_id:'default',name:'延迟关闭下装',category:'bottom'});
 await call('outfit.create',{member_id:'default',name:'延迟关闭验收',items:[{clothing_id:top.id,role:'top',layer:0},{clothing_id:bottom.id,role:'bottom',layer:1}]});
 await page.goto('/');
 await expect(page.locator('.collection .grid article').first()).toBeVisible();
 await page.getByRole('navigation').getByRole('button',{name:'穿搭',exact:true}).click();
 await page.getByRole('button',{name:'编辑 延迟关闭验收',exact:true}).click();
 const dialog=page.getByRole('dialog',{name:'调整穿搭'});
 await expect(dialog.getByLabel('搭配名称',{exact:true})).toHaveValue('延迟关闭验收');
 let release:()=>void=()=>{};
 const gate=new Promise<void>(resolve=>{release=resolve;});
 await page.route('**/__wardrobe_dev_api',async route=>{
  const body=route.request().postData()??'';
  if(body.includes('outfit.delete')){await gate;await route.continue();return;}
  await route.continue();
 });
 await dialog.getByRole('button',{name:'移入搭配回收站',exact:true}).click();
 await dialog.getByRole('button',{name:'确认移入回收站',exact:true}).click();
 const close=dialog.getByRole('button',{name:'关闭单品窗口'});
 await expect(close).toBeDisabled();
 await expect(dialog.getByRole('button',{name:'确认移入回收站',exact:true})).toBeDisabled();
 await expect(dialog).toBeVisible();
 release();
 await expect(close).toBeEnabled();
 await close.click();
 await expect(page.getByRole('dialog',{name:'调整穿搭'})).toHaveCount(0);
 await page.getByRole('button',{name:'回收站',exact:true}).click();
 await expect(page.getByRole('button',{name:'编辑 延迟关闭验收',exact:true})).toBeVisible();
});
