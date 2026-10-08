import {test,expect} from '@playwright/test';
import {existsSync,readFileSync} from 'node:fs';
const nav=(page:any,name:string)=>page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name,exact:true});
const openDiagnostics=async(page:any)=>{
 await page.goto('/');await nav(page,'设置').click();
 const hub=page.getByRole('region',{name:'设置目录'});
 await expect(hub.locator('.settings-card')).toHaveCount(7);
 await hub.getByRole('button',{name:/诊断/}).click();
 const panel=page.locator('#diagnostics-settings');
 await expect(panel).toBeVisible();
 await expect(panel.locator('.diagnostics-metrics')).toBeVisible();
 return panel;
};
const noHorizontalScroll=async(page:any)=>{
 for(const width of [375,768,1440]){await page.setViewportSize({width,height:900});await expect.poll(()=>page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);}
};
test('diagnostics card opens health summary, export scopes and log directory fallback',async({page})=>{
 const panel=await openDiagnostics(page);
 const metrics=panel.locator('.diagnostics-metrics');
 await expect(metrics).toContainText('数据库模式');
 await expect(metrics).toContainText('运行会话');
 await expect(metrics).toContainText('失败任务');
 await expect(metrics).toContainText('结果未知');
 await expect(metrics).toContainText('日志占用');
 await expect(metrics.locator('.diagnostics-metric')).toHaveCount(5);
 // 打开日志目录：dev 回退显示数据目录下的 logs 路径
 await panel.getByRole('button',{name:'打开日志目录'}).click();
 await expect(panel.locator('.diagnostics-logs-path')).toContainText('/logs');
 // 导出范围四选一：session / recent / all / 显式时间窗
 const scopes=panel.getByRole('group',{name:'导出范围'});
 await expect(scopes.getByRole('radio')).toHaveCount(4);
 await expect(scopes.getByRole('radio',{name:'最近 24 小时'})).toBeChecked();
 await scopes.getByRole('radio',{name:'指定时间窗'}).check();
 await expect(panel.getByLabel('开始时间')).toBeVisible();
 await expect(panel.getByLabel('结束时间')).toBeVisible();
 await scopes.getByRole('radio',{name:'最近 24 小时'}).check();
 // dev 回退导出：写入数据目录 exports/，界面显示路径
 await panel.getByRole('button',{name:'导出日志文件'}).click();
 const result=panel.locator('.diagnostics-export-result');
 await expect(result).toBeVisible({timeout:15000});
 const text=await result.textContent();
 const token=text?.split(/\s+/).find(part=>part.includes('/exports/')&&part.endsWith('.jsonl'));
 const file=token&&token.slice(token.indexOf('/'));
 expect(file,'导出结果应显示 exports/ 下的文件路径').toBeTruthy();
 expect(file?.startsWith(process.env.WARDROBE_TEST_DATA_DIR??''),'路径应位于测试数据目录内').toBe(true);
 expect(existsSync(file),'导出文件应真实存在').toBe(true);
 const firstLine=readFileSync(file,'utf8').split('\n')[0];
 expect(JSON.parse(firstLine).kind).toBe('diagnostics');
 await expect(panel.getByRole('button',{name:'打开所在文件夹'})).toHaveCount(0);
 await noHorizontalScroll(page);
});
test('failed operation shows in recent errors and expands trace events',async({page})=>{
 const panel=await openDiagnostics(page);
 const failed=await page.request.post('/__wardrobe_dev_api',{data:{operation:'clothing.get',input:{member_id:'default',id:'diagnostics-missing-item'}}});
 expect((await failed.json()).error.code).toBe('not_found');
 await panel.getByRole('button',{name:'刷新诊断'}).click();
 const errors=panel.locator('.diagnostics-errors .diagnostics-error');
 await expect(errors.first()).toContainText('clothing.get');
 await expect(errors.first()).toContainText('not_found');
 await expect(errors.first().locator('.diagnostics-session-badge')).toContainText('更早会话');
 await errors.first().locator('summary').click();
 const trace=errors.first().locator('.diagnostics-trace');
 await expect(trace).toContainText('clothing.get');
 await expect(trace).toContainText('not_found');
 await noHorizontalScroll(page);
});
test('basic checks list renders ten passing items with a navigation link',async({page})=>{
 const panel=await openDiagnostics(page);
 const section=panel.getByRole('region',{name:'基础检查'});
 await expect(section).toBeVisible();
 const items=section.locator('.diagnostics-check');
 await expect(items).toHaveCount(10);
 // 正常环境（全新数据目录）全部通过：状态图标与状态文字并存，不只靠颜色
 await expect(section.locator('.diagnostics-check-pass')).toHaveCount(10);
 await expect(section.locator('.diagnostics-check-warn')).toHaveCount(0);
 await expect(section.locator('.diagnostics-check-fail')).toHaveCount(0);
 await expect(section.locator('.diagnostics-checks-summary')).toContainText('全部通过');
 for(const item of await items.all()){
  await expect(item.locator('.diagnostics-check-status')).toContainText(/通过|警告|失败/);
  await expect(item.locator('.diagnostics-check-detail')).toContainText(/=/);
 }
 // 去向链接：未配置模型 → 去「模型与连接」，点击跳转对应设置页（无 hover/拖拽依赖）
 const goto=section.locator('.diagnostics-check-goto');
 await expect(goto).toHaveCount(1);
 await expect(goto).toContainText('模型与连接');
 await goto.click();
 await expect(page.locator('.settings-breadcrumb')).toContainText('模型与连接');
 await noHorizontalScroll(page);
});
