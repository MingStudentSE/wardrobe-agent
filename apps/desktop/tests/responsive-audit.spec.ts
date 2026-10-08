import {test,expect,type Page} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const nav=(p:Page,name:string)=>p.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name,exact:true});
async function fits(page:Page,label:string){
 await expect.poll(()=>page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 const dialogs=await page.locator('dialog[open]').evaluateAll(els=>els.map(e=>{const r=e.getBoundingClientRect();return {top:r.top,bottom:r.bottom,height:innerHeight};}));for(const d of dialogs){expect(d.top,label).toBeGreaterThanOrEqual(0);expect(d.bottom,label).toBeLessThanOrEqual(d.height+1);}
 const issues=await page.evaluate(()=>Array.from(document.querySelectorAll('main button,main p,main h1,main h2,main h3,main small,main label,dialog button')).flatMap(el=>{
 const r=el.getBoundingClientRect(),s=getComputedStyle(el);if(!r.width||!r.height||s.visibility==='hidden'||el.closest('[hidden]'))return [];
 if(r.left < -2 || r.right > innerWidth+2)return [`${el.className||el.tagName}: outside viewport`];
 if(el.scrollWidth>el.clientWidth+2&&s.overflowX==='visible'&&s.textOverflow!=='ellipsis')return [`${el.className||el.tagName}: ${el.textContent?.slice(0,60)} (${el.scrollWidth}/${el.clientWidth})`];return [];
 }));expect(issues,label).toEqual([]);
}
test('all workspaces and settings fit intermediate and small windows',async({page})=>{
 execFileSync(process.env.WARDROBE_CLI_BIN??fileURLToPath(new URL('../../../target/debug/wardrobe-cli',import.meta.url)),['--trusted-local','clothing.create',JSON.stringify({member_id:'default',name:'长名称验收衣物春秋宽松休闲纯棉连帽外套适合日常出门通勤穿着',category:'top'})],{env:{...process.env,WARDROBE_DATA_DIR:process.env.WARDROBE_TEST_DATA_DIR}});
 await page.emulateMedia({reducedMotion:'reduce'});await page.goto('/');
 for(const [width,height] of [[375,900],[667,900],[768,900],[900,900],[1024,900],[1440,900],[900,500]]){
 await page.setViewportSize({width,height});
 for(const name of ['衣橱','穿搭','记录','设置']){await nav(page,name).click();await fits(page,`${name} ${width}`);}
 await nav(page,'穿搭').click();await page.getByRole('button',{name:'帮我搭配',exact:true}).click();await fits(page,`搭配推荐 ${width}`);
 await nav(page,'记录').click();for(const tab of ['衣橱洞察','洗护记录','穿着记录']){await page.getByRole('button',{name:tab,exact:true}).click();await fits(page,`${tab} ${width}`);}
 await page.getByRole('button',{name:'＋ 记录穿着',exact:true}).click();await page.getByRole('dialog').getByLabel('搜索衣物').fill('长名称验收');await expect(page.getByRole('dialog').getByRole('button',{name:'选择 长名称验收衣物春秋宽松休闲纯棉连帽外套适合日常出门通勤穿着',exact:true})).toBeVisible();await fits(page,`记录弹窗 ${width}`);await page.getByRole('dialog').getByRole('button',{name:'关闭单品窗口'}).click();
 await nav(page,'衣橱').click();
 await page.getByRole('button',{name:'地点',exact:true}).click();await fits(page,`地点弹窗 ${width}`);await page.getByRole('button',{name:'手填坐标',exact:true}).click();await fits(page,`地点坐标 ${width}`);await page.getByRole('button',{name:'关闭地点选择'}).click();
 await page.getByRole('button',{name:'筛选',exact:true}).click();await page.getByText('更多条件',{exact:false}).click();await fits(page,`筛选弹窗 ${width}`);await page.getByRole('button',{name:'关闭筛选'}).click();
 await page.getByRole('button',{name:'＋ 管理分类',exact:true}).click();await fits(page,`分类管理 ${width}`);await page.getByRole('button',{name:'关闭分类管理'}).click();
 await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();await fits(page,`新增弹窗 ${width}`);await page.getByRole('dialog').getByRole('button',{name:'关闭单品窗口'}).click();
 await page.locator('.garment-open').first().click();await fits(page,`单品详情 ${width}`);await page.getByRole('dialog').getByRole('button',{name:'编辑衣物',exact:true}).click();for(const tab of ['基本资料','照片管理','历史与管理']){await page.getByRole('dialog').getByRole('button',{name:tab,exact:true}).click();await fits(page,`编辑${tab} ${width}`);}await page.getByRole('dialog').getByRole('button',{name:'关闭单品窗口'}).click();
 await nav(page,'设置').click();
 const hub=page.getByRole('region',{name:'设置目录'});
 for(const name of ['穿衣偏好','模型与连接','任务中心','备份与恢复','加密与设备','CLI / MCP','诊断']){await hub.getByRole('button',{name:new RegExp(name)}).click();await fits(page,`${name} ${width}`);
 if(['备份与恢复','加密与设备','CLI / MCP'].includes(name)){for(const summary of await page.locator('.view-panel:not([hidden]) details:not([open])>summary').all()){if(await summary.isVisible())await summary.click();}await fits(page,`${name}展开 ${width}`);}
 if(name==='模型与连接'||name==='穿衣偏好'){await page.getByRole('button',{name:name==='模型与连接'?'＋ 添加模型':'＋ 添加偏好',exact:true}).click();await fits(page,`${name}弹窗 ${width}`);await page.getByRole('dialog').getByRole('button',{name:'关闭单品窗口'}).click();}
 await page.getByRole('button',{name:'← 返回设置'}).click();}
 }
});
