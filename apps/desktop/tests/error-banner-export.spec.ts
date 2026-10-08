import {test,expect} from '@playwright/test';
import {existsSync,readFileSync} from 'node:fs';
const noHorizontalScroll=async(page:any)=>{
 for(const width of [375,768,1440]){await page.setViewportSize({width,height:900});await expect.poll(()=>page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);}
};
test('失败操作后错误反馈条就地「导出日志」，dev 回退导出并保持「重新读取」可用',async({page})=>{
 // 制造一次真实失败：开发桥对 clothing.list 前两次请求返回其自身错误契约（503 + error JSON），
 // App.start() 捕获后置入全局错误反馈条；WardrobeOverview 的同名单独失败不影响全局条。
 let failedLists=0;
 await page.route('**/__wardrobe_dev_api',async route=>{
  const request=route.request();
  let operation='';try{operation=(JSON.parse(request.postData()??'{}')as{operation?:string}).operation??'';}catch{}
  if(operation==='clothing.list'&&failedLists<2){failedLists+=1;await route.fulfill({status:503,contentType:'application/json',body:JSON.stringify({error:{code:'dev_bridge_unavailable',message:'暂时无法读取衣橱，请重试。'}})});return;}
  await route.continue();
 });
 await page.goto('/');
 const banner=page.locator('main div.feedback.error[role="alert"]');
 await expect(banner).toBeVisible();
 await expect(banner).toContainText('暂时无法读取衣橱，请重试。');
 // 反馈条内出现「导出日志」次级按钮，与「重新读取」并列，Tab 顺序自然可达
 const exportButton=banner.getByRole('button',{name:'导出日志'});
 await expect(banner.getByRole('button',{name:'重新读取'})).toBeVisible();
 await expect(exportButton).toBeVisible();
 await exportButton.click();
 // 固定 recent 范围：成功后就地显示路径 + 事件数（role=status），dev 无「打开所在文件夹」
 const result=page.locator('main div[role="status"].diagnostics-export-result');
 await expect(result).toBeVisible({timeout:15000});
 await expect(result).toContainText('已导出');
 await expect(result).toContainText('个事件');
 await expect(result.getByRole('button',{name:'打开所在文件夹'})).toHaveCount(0);
 const text=await result.textContent();
 const token=text?.split(/\s+/).find(part=>part.includes('/exports/')&&part.endsWith('.jsonl'));
 const file=token&&token.slice(token.indexOf('/'));
 expect(file,'导出结果应显示 exports/ 下的文件路径').toBeTruthy();
 expect(file?.startsWith(process.env.WARDROBE_TEST_DATA_DIR??''),'路径应位于测试数据目录内').toBe(true);
 expect(file?.match(/衣橱日志-\d{8}-\d{4}\.jsonl$/),'默认文件名应为 衣橱日志-YYYYMMDD-HHmm.jsonl').toBeTruthy();
 expect(existsSync(file),'导出文件应真实存在').toBe(true);
 const meta=JSON.parse(readFileSync(file!,'utf8').split('\n')[0]);
 expect(meta.kind).toBe('diagnostics');
 // 固定 recent：meta 标注 scope=recent，覆盖窗口落在最近 24 小时内
 expect(meta.scope).toBe('recent');
 const now=Date.now();
 expect(Date.parse(meta.exported_at)).toBeGreaterThan(now-60_000);
 expect(Date.parse(meta.covered_to)).toBeGreaterThan(now-60_000);
 expect(Date.parse(meta.covered_from)).toBeGreaterThanOrEqual(now-24*60*60_000);
 expect(Date.parse(meta.covered_from)).toBeLessThanOrEqual(now);
 expect(meta.event_count).toBeGreaterThan(0);
 // 三档宽度无横向滚动（反馈条与导出结果同时在场时）
 await noHorizontalScroll(page);
 // 导出失败不影响原有「重新读取」按钮：拦截已放行，点击后恢复并收起反馈条
 await banner.getByRole('button',{name:'重新读取'}).click();
 await expect(banner).toBeHidden({timeout:15000});
});
