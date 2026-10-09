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
test('记录复现全流程：开始 → 真实操作 → 停止导出，产物含操作时间线与 kind=reproduction',async({page})=>{
 const panel=await openDiagnostics(page);
 const section=panel.getByRole('region',{name:'记录复现'});
 await expect(section).toBeVisible();
 // 开始记录：出现录制中状态（role=status，红点 + 文字 + 计时双通道）
 await section.getByRole('button',{name:'开始记录复现'}).click();
 const status=section.locator('.diagnostics-recording');
 await expect(status).toBeVisible();
 await expect(status).toContainText('正在记录操作');
 // 真实操作：切到「穿搭」视图再进「穿搭库」分区（视图/分区切换插桩点）
 await nav(page,'穿搭').click();
 await page.locator('.workspace-tabs').getByRole('button',{name:'穿搭库'}).click();
 // 回到诊断页：录制状态保留（视图切换不卸载面板，仅页面刷新才放弃）
 await nav(page,'设置').click();
 await expect(section.locator('.diagnostics-recording')).toBeVisible();
 // 停止并导出：复用共享导出助手交互，显示路径（dev 回退为数据目录 exports/）
 await section.getByRole('button',{name:'停止并导出'}).click();
 const result=section.locator('.diagnostics-export-result');
 await expect(result).toBeVisible({timeout:15000});
 await expect(result).toContainText('复现记录已导出');
 const text=await section.textContent();
 const token=text?.split(/\s+/).find(part=>part.includes('/exports/')&&part.endsWith('.jsonl'));
 const file=token&&token.slice(token.indexOf('/'));
 expect(file,'复现记录应显示 exports/ 下的文件路径').toBeTruthy();
 expect(file?.startsWith(process.env.WARDROBE_TEST_DATA_DIR??''),'路径应位于测试数据目录内').toBe(true);
 expect(file?.match(/复现记录-\d{8}-\d{4}\.jsonl$/),'默认文件名应为 复现记录-YYYYMMDD-HHmm.jsonl').toBeTruthy();
 expect(existsSync(file),'导出文件应真实存在').toBe(true);
 // node fs 断言产物：meta.kind=reproduction + checks + 完整操作时间线（标记 + ui 事件 + 操作事件）
 const lines=readFileSync(file!,'utf8').split('\n').filter(line=>line.trim());
 const meta=JSON.parse(lines[0]);
 expect(meta.kind).toBe('reproduction');
 expect(meta.checks).toBeTruthy();
 expect(meta.checks.checks).toHaveLength(10);
 expect(meta.checks.counts.pass).toBe(10);
 const events=lines.slice(1).map(line=>JSON.parse(line));
 const markers=events.filter((event:any)=>event.event_type==='reproduction');
 expect(markers.map((marker:any)=>marker.phase)).toEqual(['start','stop']);
 expect(events.find((event:any)=>event.event_type==='ui'&&event.type==='view_switch'&&event.label==='穿搭'),'产物须含切到穿搭的 ui 事件').toBeTruthy();
 expect(events.find((event:any)=>event.event_type==='ui'&&event.type==='view_switch'&&event.label==='穿搭库'),'产物须含进入穿搭库分区的 ui 事件').toBeTruthy();
 expect(events.find((event:any)=>event.operation==='diagnostics.ui_event'&&event.result==='ok'),'ui_event 操作事件同样在窗口内').toBeTruthy();
 expect(events.find((event:any)=>event.operation==='diagnostics.reproduction.start'&&event.result==='ok'),'开始标记的操作事件在窗口内').toBeTruthy();
 // 三档宽度无横向滚动（录制结束态 + 导出结果在场）
 await noHorizontalScroll(page);
});
test('注入短上限：到点自动停止并提示（role=status）',async({page})=>{
 await page.addInitScript(()=>{localStorage.setItem('wardrobe-reproduction-max-seconds','3');});
 const panel=await openDiagnostics(page);
 const section=panel.getByRole('region',{name:'记录复现'});
 await section.getByRole('button',{name:'开始记录复现'}).click();
 await expect(section.locator('.diagnostics-recording')).toBeVisible();
 // 3 秒上限：自动停止提示 + 导出结果出现，录制状态收起
 await expect(section.locator('.diagnostics-reproduction-note')).toContainText('自动停止',{timeout:20000});
 const result=section.locator('.diagnostics-export-result');
 await expect(result).toBeVisible({timeout:15000});
 await expect(section.locator('.diagnostics-recording')).toHaveCount(0);
 // 自动停止产物同样是复现记录（meta.kind=reproduction 且附 checks）
 const text=await section.textContent();
 const token=text?.split(/\s+/).find(part=>part.includes('/exports/')&&part.endsWith('.jsonl'));
 const file=token&&token.slice(token.indexOf('/'));
 expect(file&&existsSync(file),'自动停止也应成功导出文件').toBe(true);
 const meta=JSON.parse(readFileSync(file!,'utf8').split('\n')[0]);
 expect(meta.kind).toBe('reproduction');
 expect(meta.checks).toBeTruthy();
});
