import {test,expect} from '@playwright/test';
// 模型连接验收（新 UI）：设置 → 模型与连接；配置以卡片列表呈现，
// 新增/管理连接用弹窗。行为断言（可重读、能力勾选、内置测试图验证）保持不变。
async function openModelSettings(page:any){
 await page.goto('/');
 await page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name:'设置',exact:true}).click();
 await page.getByRole('region',{name:'设置目录'}).getByRole('button',{name:/模型与连接/}).click();
 return page.getByRole('region',{name:'模型设置'});
}
test('模型配置可重读，无密钥不会阻断已有衣橱',async({page})=>{
 const settings=await openModelSettings(page);
 await settings.getByRole('button',{name:'＋ 添加模型'}).click();
 const dialog=page.getByRole('dialog');
 await dialog.getByLabel('配置名称').fill('本机测试模型');
 await dialog.getByLabel('兼容 API 端点').fill('http://127.0.0.1:11434/v1');
 await dialog.getByLabel('模型名称').fill('local-fixture');
 await dialog.getByLabel('认证方式').selectOption('none');
 await dialog.getByRole('button',{name:'保存模型配置'}).click();
 await expect(dialog.getByRole('status').filter({hasText:'模型配置已保存'})).toBeVisible();
 await dialog.getByRole('button',{name:'关闭单品窗口'}).click();
 await page.reload();
 const reloaded=await openModelSettings(page);
 await reloaded.locator('.provider-grid article').filter({hasText:'本机测试模型'}).getByRole('button',{name:'管理连接'}).click();
 const readback=page.getByRole('dialog');
 await expect(readback.getByLabel('模型名称')).toHaveValue('local-fixture');
 await expect(readback.getByLabel('兼容 API 端点')).toHaveValue('http://127.0.0.1:11434/v1');
 await readback.getByRole('button',{name:'关闭单品窗口'}).click();
 for(const width of [320,375,414,768,1024,1280,1440]){await page.setViewportSize({width,height:900});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBeTruthy();if(width===320||width===1280)await page.locator('#model-settings').screenshot({path:`../../docs/verification/ticket-12/model-settings-${width}.png`});}
 // 模型配置异常不阻断衣橱：返回衣橱工作区可见且库存可读。
 await page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name:'衣橱',exact:true}).click();
 await expect(page.getByRole('heading',{name:'我的衣橱',exact:true})).toBeVisible();
 await expect(page.locator('.collection .grid article').first()).toBeVisible();
});

test('视觉专用配置仅在明确点击后发送内置测试图',async({page})=>{
 const {createServer}=await import('node:http');
 const requests:unknown[]=[];
 const server=createServer((req,res)=>{let body='';req.on('data',chunk=>body+=chunk);req.on('end',()=>{requests.push(JSON.parse(body));res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify({choices:[{message:{content:'Blue square on light background'}}]}));});});
 await new Promise<void>(resolve=>server.listen(0,'127.0.0.1',resolve));
 try{
 const address=server.address() as {port:number};
 const settings=await openModelSettings(page);
 await settings.getByRole('button',{name:'＋ 添加模型'}).click();
 const dialog=page.getByRole('dialog');
 await dialog.getByLabel('配置名称').fill('视觉连接测试');
 await dialog.getByLabel('兼容 API 端点').fill(`http://127.0.0.1:${address.port}/v1`);
 await dialog.getByLabel('模型名称').fill('vision-fixture');
 await dialog.getByRole('checkbox',{name:'文本',exact:true}).uncheck();
 await dialog.getByRole('checkbox',{name:'视觉',exact:true}).check();
 await dialog.getByLabel('认证方式').selectOption('none');
 await dialog.getByRole('button',{name:'保存模型配置'}).click();
 await expect(dialog.getByLabel('待验证能力')).toHaveValue('vision');expect(requests).toHaveLength(0);
 await dialog.getByRole('button',{name:'验证所选能力'}).click();await expect(dialog.getByRole('status').filter({hasText:'视觉连接验证成功'})).toBeVisible();expect(requests).toHaveLength(1);
 const request=requests[0] as any;expect(request.messages[0].content[1].image_url.url).toMatch(/^data:image\/png;base64,/);expect(request.model).toBe('vision-fixture');
 for(const width of [320,375,414,768,1024,1280,1440]){await page.setViewportSize({width,height:900});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBeTruthy();if(width===320||width===1280)await page.locator('#model-settings').screenshot({path:`../../docs/verification/ticket-12-vision/model-settings-${width}.png`});}
 }finally{await new Promise<void>(resolve=>server.close(()=>resolve()));}
});
