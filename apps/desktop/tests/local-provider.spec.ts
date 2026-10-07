import {test,expect} from '@playwright/test';
// Opt-in real model test; no intercepted routes or fake model response.
test('本地真实 SmolVLM 文本与视觉连接通过 GUI 和共享 CLI',async({page})=>{
 test.skip(!process.env.WARDROBE_REAL_LOCAL_PROVIDER,'Requires explicitly started real loopback model');
 test.setTimeout(90_000);
 const endpoint=process.env.WARDROBE_REAL_LOCAL_PROVIDER!;
 expect(endpoint).toMatch(/^http:\/\/127\.0\.0\.1:\d+\/v1$/);
 await page.goto('/');await page.getByText('模型设置',{exact:true}).click();
 await page.getByLabel('配置名称').fill('真实本地 SmolVLM 256M');await page.getByLabel('兼容 API 端点').fill(endpoint);await page.getByLabel('模型名称').fill('wardrobe-smolvlm');
 await page.getByRole('checkbox',{name:'视觉',exact:true}).check();await page.getByLabel('认证方式').selectOption('none');await page.getByRole('button',{name:'保存模型配置'}).click();
 await expect(page.getByLabel('待验证能力')).toHaveValue('text');
 for(const [capability,label] of [['text','文本'],['vision','视觉']]){
 await page.getByLabel('待验证能力').selectOption(capability);await page.getByRole('button',{name:'验证所选能力'}).click();await expect(page.getByRole('status').filter({hasText:`${label}连接验证成功`})).toBeVisible({timeout:25_000});
 }
 for(const width of [320,375,414,768,1024,1280,1440]){await page.setViewportSize({width,height:900});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBeTruthy();if(width===320||width===1280)await page.locator('#model-settings').screenshot({path:`../../docs/verification/ticket-12-local/real-local-${width}.png`});}
});
