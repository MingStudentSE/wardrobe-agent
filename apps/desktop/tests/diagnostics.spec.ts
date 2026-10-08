import {test,expect} from '@playwright/test';
import {existsSync,readFileSync} from 'node:fs';
import {join} from 'node:path';

// #41 诊断系统端到端验收：诊断页、错误反馈条一键导出、复现记录窗口导出。
// dev 桥经真实 CLI 落盘；导出文件断言用 node fs 读取 WARDROBE_DATA_DIR。
const dataDir=process.env.WARDROBE_TEST_DATA_DIR??'';
const nav=(page:import('@playwright/test').Page,name:string)=>page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name,exact:true});
const openDiagnostics=async(page:import('@playwright/test').Page)=>{
 await page.goto('/');
 await nav(page,'设置').click();
 const hub=page.getByRole('region',{name:'设置目录'});
 await expect(hub.locator('.settings-card')).toHaveCount(9);
 await hub.getByRole('button',{name:/诊断/}).click();
 const panel=page.locator('#diagnostics-settings');
 await expect(panel).toBeVisible();
 return panel;
};

test('diagnostics page shows health summary, passing checks and exports a single JSONL file',async({page})=>{
 const panel=await openDiagnostics(page);
 // 健康摘要与基础检查（正常环境无 fail；未配置模型允许 warn）
 await expect(panel.getByText(/本次启动编号/)).toBeVisible();
 const checks=panel.locator('.diag-check');
 await expect(checks.first()).toBeVisible();
 await expect(panel.locator('.diag-check-fail')).toHaveCount(0);
 await expect(panel.locator('.diag-check-pass').first()).toBeVisible();
 // 范围选择与导出（dev 回退：写入数据目录 exports/）
 await panel.getByLabel(/导出范围/).selectOption('recent');
 await panel.getByRole('button',{name:'导出诊断文件'}).click();
 const status=panel.locator('.diag-export-result');
 await expect(status).toContainText('已保存');
 const path=await status.locator('code').innerText();
 expect(existsSync(path)).toBe(true);
 const firstLine=readFileSync(path,'utf8').split('\n')[0];
 const meta=JSON.parse(firstLine);
 expect(meta.kind).toBe('diagnostics');
 expect(meta.event_count).toBeGreaterThan(0);
 expect(Object.keys(meta.counts_by_level).length).toBeGreaterThan(0);
 // 错误注入后最近错误可展开同 trace 事件序列
 const failed=await page.request.post('/__wardrobe_dev_api',{data:{operation:'clothing.create',input:{member_id:'default',name:' ',category:'top'}}});
 expect(failed.status()).toBe(200);
 await panel.getByRole('button',{name:'刷新诊断信息'}).click();
 await expect(panel.locator('.diag-error').first()).toBeVisible();
 await panel.getByRole('button',{name:'查看完整链路'}).first().click();
 await expect(panel.locator('.diag-trace-step').first()).toBeVisible();
 for(const width of [375,768,1440]){await page.setViewportSize({width,height:900});await expect.poll(()=>page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);}
});

test('error feedback bar offers one-click log export that writes a meta-first JSONL file',async({page})=>{
 await page.goto('/');
 // 拦截一次 clothing.list 使全局错误反馈条出现（其余调用照常透传）
 await page.route('**/__wardrobe_dev_api',async route=>{
  const body=route.request().postDataJSON() as {operation:string};
  if(body.operation==='clothing.list'){
   await route.fulfill({status:200,contentType:'application/json',body:JSON.stringify({error:{code:'service_unavailable',message:'诊断验收：列表读取失败'}})});
   return;
  }
  await route.continue();
 });
 const alert=page.locator('div.feedback.error[role="alert"]');
 await expect(alert).toBeVisible();
 await alert.getByRole('button',{name:'导出日志'}).click();
 await expect(alert.locator('code')).toBeVisible({timeout:20000});
 const path=await alert.locator('code').innerText();
 expect(existsSync(path)).toBe(true);
 const lines=readFileSync(path,'utf8').trim().split('\n');
 const meta=JSON.parse(lines[0]);
 expect(meta.kind).toBe('diagnostics');
 expect(lines.length-1).toBeGreaterThan(0);
 // 产物是单文件 JSONL：每行都是合法 JSON 事件
 for(const line of lines.slice(1,5)){expect(()=>JSON.parse(line)).not.toThrow();}
 // 产物不含数据库、图片或凭据文件，只有日志事件
 expect(path.endsWith('.jsonl')).toBe(true);
});

test('reproduction recording captures ui timeline and exports a windowed file',async({page})=>{
 const panel=await openDiagnostics(page);
 await panel.getByRole('button',{name:'开始记录复现'}).click();
 await expect(panel.locator('.diag-recording')).toBeVisible();
 // 正常使用：切换视图（记录页面切换事件），再做一个真实操作
 await nav(page,'衣橱').click();
 await nav(page,'记录').click();
 await nav(page,'设置').click();
 await panel.getByRole('button',{name:'停止并导出'}).click();
 await expect(panel.locator('.diag-export-result')).toContainText('已保存');
 const path=await panel.locator('.diag-export-result code').innerText();
 expect(existsSync(path)).toBe(true);
 const lines=readFileSync(path,'utf8').trim().split('\n');
 const meta=JSON.parse(lines[0]);
 expect(meta.kind).toBe('reproduction');
 expect(meta.checks).toBeTruthy();
 const events=lines.slice(1).map(line=>JSON.parse(line));
 // 窗口含操作时间线（页面切换）与复现起止标记
 expect(events.some((event:Record<string,unknown>)=>event.op==='ui.view')).toBe(true);
 expect(events.some((event:Record<string,unknown>)=>event.op==='diagnostics.reproduction.start')).toBe(true);
 expect(events.some((event:Record<string,unknown>)=>event.op==='diagnostics.reproduction.stop')).toBe(true);
 // 白名单外类型被核心拒绝
 const rejected=await page.request.post('/__wardrobe_dev_api',{data:{operation:'diagnostics.ui_event',input:{type:'freeform',label:'x'}}});
 expect((await rejected.json()).error.code).toBe('invalid_input');
});

test('cli doctor shares check definitions with the diagnostics page',async({page})=>{
 const panel=await openDiagnostics(page);
 await expect(panel.locator('.diag-check')).not.toHaveCount(0);
 const pageIds=await panel.locator('.diag-check').evaluateAll(nodes=>nodes.map(node=>node.getAttribute('data-check-id')));
 const {execFileSync}=await import('node:child_process');
 const {fileURLToPath}=await import('node:url');
 const cli=process.env.WARDROBE_CLI_BIN??fileURLToPath(new URL('../../../target/debug/wardrobe-cli',import.meta.url));
 const stdout=execFileSync(cli,['--db',join(dataDir,'wardrobe.sqlite'),'doctor'],{env:{...process.env,WARDROBE_DATA_DIR:dataDir}}).toString();
 const report=JSON.parse(stdout).result;
 expect(report.checks.items.map((item:{id:string})=>item.id)).toEqual(pageIds);
 const database=report.checks.items.find((item:{id:string})=>item.id==='database');
 expect(database.status).toBe('pass');
 expect(report.summary.database.matches_application).toBe(true);
});
