import {test,expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {readFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';

// 浏览器内安全边界证据（ADR0026 D4）：撤销立即传播、CSRF/Origin 在真实浏览器中拒绝、
// /auth/state 不泄露连接令牌、连接令牌可重新登录。
const dataDir=process.env.WARDROBE_WEB_TEST_DATA!;
const cli=process.env.WARDROBE_CLI_BIN??fileURLToPath(new URL('../../../target/debug/wardrobe-cli',import.meta.url));
const trusted=(operation:string,input:object={})=>JSON.parse(
 execFileSync(cli,['--trusted-local',operation,JSON.stringify(input)],{env:{...process.env,WARDROBE_DATA_DIR:dataDir}}).toString(),
).result;

test('connection token logs a second browser context in',async({browser})=>{
 const connection=readFileSync(fileURLToPath(new URL('./.auth/connection-token.txt',import.meta.url)),'utf8').trim();
 const context=await browser.newContext({storageState:{cookies:[],origins:[]}});
 const page=await context.newPage();
 await page.goto('/');
 const probe=await page.evaluate(async ()=>{const r=await fetch('/auth/state');return {text:await r.text(),cookies:document.cookie};});
 await page.getByLabel('连接令牌').fill(connection);
 await page.getByRole('button',{name:'登录'}).click();
 await expect(page.getByRole('heading',{name:'我的衣橱'})).toBeVisible();
 await context.close();
});

test('session API refuses forged cross-site requests inside a real browser',async({page})=>{
 await page.goto('/');
 await expect(page.getByRole('heading',{name:'我的衣橱'})).toBeVisible();
 // 同源但缺少 CSRF 头（模拟第三方页面无法拿到的头）：403
 const noCsrf=await page.evaluate(async ()=>{
  const r=await fetch('/api',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({operation:'capabilities',input:{}})});
  return {status:r.status};
 });
 expect(noCsrf.status).toBe(403);
 // page.request（无 Origin 头的客户端）：403
 const noOrigin=await page.request.post('/api',{data:{operation:'capabilities',input:{}},headers:{'Cookie':(await page.context().cookies()).map(c=>`${c.name}=${c.value}`).join('; ')}});
 expect(noOrigin.status()).toBe(403);
 // /auth/state 只暴露 csrf 与状态，绝不暴露连接令牌
 const state=await page.evaluate(async ()=>await (await fetch('/auth/state')).json());
 expect(state.result.authenticated).toBe(true);
 expect(state.result.token).toBeUndefined();
 expect(String(JSON.stringify(state))).not.toMatch(/"token":"[0-9a-f]{64}"/);
 // 特权操作经会话 API 仍被核心拒绝
 const privileged=await page.evaluate(async ()=>{
  const state=await (await fetch('/auth/state')).json();
  return await (await fetch('/api',{method:'POST',headers:{'Content-Type':'application/json','X-Wardrobe-CSRF':state.result.csrf},body:JSON.stringify({operation:'client.authorize',input:{name:'x',mode:'full',operations:[],member_ids:null,expires_at:null}})})).json();
 });
 expect(privileged.error.code).toBe('scope_denied');
});

test('revoking the browser grant propagates to the live session immediately',async({page})=>{
 await page.goto('/');
 await expect(page.getByRole('heading',{name:'我的衣橱'})).toBeVisible();
 const clients=trusted('client.list');
 const browserClient=clients.items.find((candidate:{name:string})=>candidate.name==='Web 浏览器');
 expect(browserClient,'应存在 Web 浏览器客户端授权').toBeTruthy();
 trusted('client.revoke',{id:browserClient.id});
 // 已登录页面的下一个请求立即 401，界面回到登录门
 await page.evaluate(async ()=>await fetch('/auth/state'));
 await page.reload();
 await expect(page.getByRole('heading',{name:'登录本机衣橱'})).toBeVisible();
 // 撤销传播到核心：即使会话仍持有 cookie+csrf，核心也返回 authorization_revoked
 const denied=await page.evaluate(async ()=>{
  const state=await (await fetch('/auth/state')).json();
  const r=await fetch('/api',{method:'POST',headers:{'Content-Type':'application/json','X-Wardrobe-CSRF':state.result.csrf},body:JSON.stringify({operation:'capabilities',input:{}})});
  return await r.json();
 });
 expect(denied.error.code).toBe('authorization_revoked');
});
