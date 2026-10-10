import {test,expect} from '@playwright/test';
import {mkdirSync,readFileSync,writeFileSync} from 'node:fs';
import {dirname,join} from 'node:path';
import {fileURLToPath} from 'node:url';

// 首启引导：一次性引导令牌 → 恰好一次创建浏览器授权 → 会话建立（ADR0029 D4）。
const dataDir=process.env.WARDROBE_WEB_TEST_DATA!;
const authFile=fileURLToPath(new URL('./.auth/wardrobe.json',import.meta.url));
const connectionFile=fileURLToPath(new URL('./.auth/connection-token.txt',import.meta.url));

test('bootstrap with one-time local token reaches the wardrobe UI and saves the connection token once',async({page})=>{
 await page.goto('/');
 await expect(page.getByRole('heading',{name:'首次使用：登记本机浏览器'})).toBeVisible();
 // 引导令牌路径只提示本机操作员；页面本身不显示令牌内容
 const token=readFileSync(join(dataDir,'web-bootstrap-token'),'utf8').trim();
 await page.getByLabel('引导令牌').fill(token);
 await page.getByRole('button',{name:'登记本机浏览器'}).click();
 const tokenBox=page.getByLabel('本浏览器的连接令牌（只显示这一次）');
 await expect(tokenBox).toBeVisible();
 const connection=await tokenBox.inputValue();
 expect(connection).toHaveLength(64);
 mkdirSync(dirname(connectionFile),{recursive:true});
 writeFileSync(connectionFile,connection);
 await page.getByRole('button',{name:'我已保存，进入衣橱'}).click();
 await expect(page.getByRole('heading',{name:'我的衣橱'})).toBeVisible();
 // 会话 Cookie 属性：HttpOnly + SameSite=Strict（真实浏览器证据）
 const cookie=(await page.context().cookies()).find(candidate=>candidate.name==='wardrobe_session');
 expect(cookie, '应存在 wardrobe_session 会话 Cookie').toBeTruthy();
 expect(cookie!.httpOnly).toBe(true);
 expect(cookie!.sameSite).toBe('Strict');
 await page.context().storageState({path:authFile});
});

test('bootstrap token is consumed and cannot be reused from another browser',async({browser})=>{
 const connection=readFileSync(connectionFile,'utf8').trim();
 const context=await browser.newContext({storageState:{cookies:[],origins:[]}});
 const page=await context.newPage();
 await page.goto('/');
 // 引导令牌已消费：直接进入登录页，而不是首启页
 await expect(page.getByRole('heading',{name:'登录本机衣橱'})).toBeVisible();
 const response=await page.evaluate(async body=>{const r=await fetch('/auth/bootstrap',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({token:body})});return {status:r.status};},connection);
 expect(response.status).toBe(403);
 // 引导令牌文件本身已删除
 expect(()=>readFileSync(join(dataDir,'web-bootstrap-token'),'utf8')).toThrow();
 await context.close();
});
