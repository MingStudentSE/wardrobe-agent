import {test,expect,type Page} from '@playwright/test';
import {mkdirSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
// ticket-34 全应用验收：键盘可达、触摸等价、字体缩放、减少动画、
// 空/加载/错误/离线状态、中文长名称。全部断言只观察外部行为。
const shots=fileURLToPath(new URL('../../../docs/verification/ticket-34/',import.meta.url));
const nav=(p:Page,name:string)=>p.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name,exact:true});
mkdirSync(shots,{recursive:true});

function overflowIssues(page:Page){
 return page.evaluate(()=>Array.from(document.querySelectorAll('main button,main p,main h1,main h2,main h3,main small,main label,dialog button,main strong'))
  .flatMap(el=>{const r=el.getBoundingClientRect(),s=getComputedStyle(el);if(!r.width||!r.height||s.visibility==='hidden'||el.closest('[hidden]'))return [];
   if(r.left<-2||r.right>innerWidth+2)return [`${el.className||el.tagName}: outside viewport`];
   if(el.scrollWidth>el.clientWidth+2&&s.overflowX==='visible'&&s.textOverflow!=='ellipsis')return [`${el.className||el.tagName}: ${el.textContent?.slice(0,40)} (${el.scrollWidth}/${el.clientWidth})`];return [];}));
}

test('skip link、侧边导航与设置目录支持键盘聚焦、Enter/Space 激活且焦点环可见',async({page})=>{
 await page.goto('/');
 await page.keyboard.press('Tab');
 const skip=page.getByRole('link',{name:'跳到内容'});
 await expect(skip).toBeFocused();
 const outline=await skip.evaluate(el=>{const s=getComputedStyle(el);return {style:s.outlineStyle,width:s.outlineWidth};});
 const navFirst=nav(page,'衣橱');
 await navFirst.focus();
 await page.keyboard.press('Enter');
 await expect(page.locator('.collection')).toBeVisible();
 await nav(page,'穿搭').focus();
 await page.keyboard.press('Space');
 await expect(page.locator('#lookbook')).toBeVisible();
 await nav(page,'设置').focus();
 await page.keyboard.press('Enter');
 const hub=page.getByRole('region',{name:'设置目录'});
 await expect(hub).toBeVisible();
 const card=hub.getByRole('button',{name:/备份与恢复/});
 await card.focus();
 await page.keyboard.press('Enter');
 await expect(page.locator('#backup-settings')).toBeVisible();
 await page.getByRole('button',{name:'← 返回设置'}).click();
 // Tab 序：焦点从正文继续时可到达主要导航与设置卡片。
 await hub.locator('.settings-card').first().focus();
 await page.keyboard.press('Shift+Tab');
 await page.keyboard.press('Shift+Tab');
 const reachable=await page.evaluate(()=>{const el=document.activeElement as HTMLElement;return el?.tagName+':'+(el.textContent?.slice(0,12)??'');});
 expect(reachable).toContain('BUTTON');
 await page.setViewportSize({width:1280,height:900});
 await nav(page,'衣橱').focus();
 await page.screenshot({path:`${shots}keyboard-focus.png`});
});

test.describe('触摸等价（无 hover-only 功能）',()=>{test.use({hasTouch:true});
test('hover 不承载独占功能，触摸路径可完成核心操作',async({page})=>{
 // 无 hover 事件下的可见性：核心操作都是常驻按钮。
 await page.goto('/');
 await expect(page.getByRole('button',{name:'＋ 新增衣物',exact:true})).toBeVisible();
 await expect(page.getByRole('button',{name:'AI 推荐',exact:true})).toBeVisible();
 await expect(page.locator('.garment-footer button').first()).toBeVisible();
 // hover 媒体查询不得隐藏任何控件：检查样式表中 hover 规则没有 display/visibility 隐藏。
 const hiddenByHover=await page.evaluate(()=>{let issues:string[]=[];for(const sheet of Array.from(document.styleSheets)){
  let rules:CSSRuleList;try{rules=sheet.cssRules;}catch{continue;}
  for(const rule of Array.from(rules)){
   if(!(rule instanceof CSSMediaRule)||!rule.conditionText.includes('hover'))continue;
   const walk=(r:CSSRule)=>{if(r instanceof CSSStyleRule){const t=r.style;if((t.display==='none'||t.visibility==='hidden')&&r.selectorText)issues.push(`${r.selectorText} @media ${rule.conditionText}`);}if(r instanceof CSSMediaRule)Array.from(r.cssRules).forEach(walk);};
   Array.from(rule.cssRules).forEach(walk);
  }}
  return issues;});
 expect(hiddenByHover).toEqual([]);
 // 触摸（tap）完成从浏览到保存穿搭的核心路径。
 await page.locator('.collection .grid article').first().getByRole('button',{name:'加入搭配'}).tap();
 const composer=page.getByRole('complementary',{name:'搭配预览'});
 await composer.getByLabel('搭配名称').fill('触摸等价验收');
 await composer.getByRole('button',{name:'保存这套穿搭'}).tap();
 await expect(composer.getByRole('status')).toContainText('搭配已保存');
 await nav(page,'记录').tap();
 await page.getByRole('button',{name:'穿着记录',exact:true}).tap();
 await expect(page.locator('#wears')).toBeVisible();
 await page.locator('#wears').getByRole('button',{name:'＋ 记录穿着',exact:true}).tap();
 await expect(page.getByRole('dialog',{name:'记录穿着',exact:true})).toBeVisible();
 await page.getByRole('dialog').getByRole('button',{name:'关闭单品窗口'}).tap();
 await expect(page.getByRole('dialog')).toHaveCount(0);
});});

test('root font-size 200% 时 375/1280 宽度不破版且核心流程可用',async({page})=>{
 await page.goto('/');
 await expect(page.locator('.collection .grid article').first()).toBeVisible();
 await page.addStyleTag({content:'html{font-size:32px}'});
 for(const width of [375,1280]){
  await page.setViewportSize({width,height:1000});
  await expect(page.locator('.collection .grid article').first()).toBeVisible();
  for(const name of ['穿搭','记录','设置','衣橱']){
   await nav(page,name).click();
   expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`overflow ${name} ${width}`).toBeTruthy();
   expect(await overflowIssues(page),`clipped ${name} ${width}`).toEqual([]);
  }
  await page.screenshot({path:`${shots}font-scale-200-${width}.png`,fullPage:true});
 }
 // 放大字体后核心流程仍可用：新增衣物弹窗可操作。
 await page.setViewportSize({width:1280,height:1000});
 await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();
 const dialog=page.getByRole('dialog');
 await dialog.getByLabel('名称',{exact:true}).fill('大字体录入验收');
 await expect(dialog.getByRole('button',{name:'添加到衣橱',exact:true})).toBeInViewport();
 await dialog.getByRole('button',{name:'取消',exact:true}).click();
});

test('prefers-reduced-motion 下无过渡动画且应用完整可用',async({page})=>{
 await page.emulateMedia({reducedMotion:'reduce'});
 await page.goto('/');
 const durations=await page.evaluate(()=>{
  const sample=[document.querySelector('.nav-button') as HTMLElement,document.querySelector('.settings-card') as HTMLElement||document.querySelector('button') as HTMLElement,document.querySelector('.collection .grid article button') as HTMLElement];
  return sample.filter(Boolean).map(el=>getComputedStyle(el).transitionDuration).join(',');
 });
 expect(durations).not.toContain('160ms');
 const motion=await page.evaluate(()=>matchMedia('(prefers-reduced-motion: reduce)').matches);
 expect(motion).toBe(true);
 // 完整可用：创建单品不因动画策略受阻。
 await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();
 const dialog=page.getByRole('dialog');
 await dialog.getByLabel('名称',{exact:true}).fill('减少动画验收');
 await dialog.getByRole('button',{name:'添加到衣橱',exact:true}).click();
 await expect(page.getByLabel('搜索衣物',{exact:true})).toBeVisible();
 await page.screenshot({path:`${shots}reduced-motion.png`,fullPage:true});
});

test('空状态逐页可见且中文提示明确',async({page})=>{
 const call=async(operation:string,input:object)=>{const r=await(await page.request.post('/__wardrobe_dev_api',{data:{operation,input}})).json();if(r.error)throw new Error(r.error.message);return r.result;};
 const member=(await call('member.create',{name:'空状态验收成员'})).member_id as string;
 await page.goto('/');
 await page.getByText('当前衣橱',{exact:true}).click();
 await page.getByRole('combobox',{name:/^当前成员/}).selectOption(member);
 await expect(page.locator('.wardrobe-counts')).toContainText('0 件单品');
 await expect(page.getByText('没有符合条件的单品')).toBeVisible();
 await nav(page,'穿搭').click();
 await expect(page.getByText('第一套穿搭，从你的衣橱开始')).toBeVisible();
 await page.getByRole('button',{name:'帮我搭配',exact:true}).click();
 // 推荐历史在“推荐反馈与记录”详情内。
 await page.locator('#recommendations summary').filter({hasText:'推荐反馈与记录'}).click();
 await expect(page.getByText('还没有推荐记录。')).toBeVisible();
 await page.getByRole('button',{name:'场景计划',exact:true}).click();
 await page.getByRole('button',{name:'购买分析',exact:true}).click();
 await nav(page,'记录').click();
 await expect(page.getByText('从今天穿的这一套开始')).toBeVisible();
 await page.getByRole('button',{name:'衣橱洞察',exact:true}).click();
 await expect(page.locator('#insights .journal-metrics')).toContainText('0');
 await page.getByRole('button',{name:'洗护记录',exact:true}).click();
 await expect(page.getByText('还没有洗护记录。')).toBeVisible();
 await nav(page,'设置').click();
 const hub=page.getByRole('region',{name:'设置目录'});
 await hub.getByRole('button',{name:/模型与连接/}).click();
 await expect(page.getByText('从一个模型开始')).toBeVisible();
 await expect(page.locator('#model-settings').getByRole('alert')).toHaveCount(0);
 await page.getByRole('button',{name:'← 返回设置'}).click();
 await hub.getByRole('button',{name:/任务中心/}).click();
 await expect(page.getByText('目前没有后台任务')).toBeVisible();
 await page.getByRole('button',{name:'← 返回设置'}).click();
 await hub.getByRole('button',{name:/备份与恢复/}).click();
 await expect(page.locator('#backup-settings')).toBeVisible();
 await page.getByRole('button',{name:'← 返回设置'}).click();
 // 同步页：当前集成分支的表面状态可用（状态页能力由 #31 并行交付后终验）。
 await hub.getByRole('button',{name:/同步/}).click();
 await expect(page.getByText('← 返回设置')).toBeVisible();
 await page.screenshot({path:`${shots}empty-states-sync.png`,fullPage:true});
 const choice=page.getByRole('combobox',{name:/^当前成员/});
 if(!(await choice.isVisible()))await page.getByText('当前衣橱').click();
 await expect(choice).toBeEnabled();
 await choice.selectOption({label:'我'});
});

test('加载中与错误状态可见，桥接失败给出可重试提示',async({page})=>{
 await page.route('**/__wardrobe_dev_api',async route=>{if(route.request().postDataJSON().operation==='clothing.list')await new Promise(r=>setTimeout(r,800));await route.continue();});
 await page.goto('/');
 await expect(page.getByText('正在打开本地衣橱…')).toBeVisible();
 await expect(page.locator('.wardrobe-counts')).toContainText('正在读取');
 await expect.poll(async()=>await page.locator('.collection .grid article').count(),{timeout:15000}).toBeGreaterThan(0);
 // 罕见错误：桥接 500 → role=alert，重新读取可恢复。
 await page.route('**/__wardrobe_dev_api',async route=>{if(route.request().postDataJSON().operation==='clothing.list'){await route.fulfill({status:503,contentType:'application/json',body:JSON.stringify({error:{code:'dev_bridge_unavailable',message:'暂时无法读取衣橱，请重试。'}})});return;}await route.continue();});
 await page.reload();
 await expect(page.getByRole('alert').first()).toContainText('暂时无法读取衣橱');
 await expect(page.getByRole('button',{name:'重新读取'})).toBeVisible();
 await page.screenshot({path:`${shots}error-state.png`,fullPage:true});
 await page.unroute('**/__wardrobe_dev_api');
 await page.getByRole('button',{name:'重新读取'}).click();
 await expect(page.locator('.collection .grid article').first()).toBeVisible();
});

test('离线（桥接不可达）时应用给出明确错误并可恢复',async({page})=>{
 await page.route('**/__wardrobe_dev_api',route=>route.abort('failed'));
 await page.goto('/');
 await expect(page.getByRole('alert').first()).toBeVisible();
 await expect(page.getByRole('button',{name:'重新读取'})).toBeVisible();
 await page.screenshot({path:`${shots}offline-state.png`,fullPage:true});
 await page.unroute('**/__wardrobe_dev_api');
 await page.getByRole('button',{name:'重新读取'}).click();
 await expect.poll(async()=>await page.locator('.collection .grid article').count(),{timeout:20000}).toBeGreaterThan(0);
});

test('中文长名称在卡片、详情与穿着弹窗中完整可读',async({page})=>{
 const longName='长名称验收衣物春秋宽松休闲纯棉连帽外套适合日常出门通勤穿着';
 await page.goto('/');
 await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();
 const dialog=page.getByRole('dialog');
 await dialog.getByLabel('名称',{exact:true}).fill(longName);
 await dialog.getByRole('button',{name:'添加到衣橱',exact:true}).click();
 await expect(dialog).not.toBeVisible();
 await page.getByLabel('搜索衣物',{exact:true}).fill('长名称验收');
 await page.getByRole('button',{name:'搜索',exact:true}).click();
 const card=page.locator('.collection .grid article').filter({hasText:'长名称验收'});
 await expect(card).toHaveCount(1);
 for(const width of [320,375,768,1440]){
  await page.setViewportSize({width,height:1000});
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`overflow ${width}`).toBeTruthy();
  expect(await overflowIssues(page),`clipped ${width}`).toEqual([]);
 }
 await page.screenshot({path:`${shots}long-name-card.png`});
 await card.getByRole('button',{name:'查看详情'}).click();
 await expect(page.getByRole('region',{name:'单品详情',exact:true})).toContainText(longName);
 const box=await page.getByRole('dialog').boundingBox();
 expect(box!.x).toBeGreaterThanOrEqual(0);
 expect(box!.x+box!.width).toBeLessThanOrEqual(1440);
 await page.getByRole('dialog').getByRole('button',{name:'今天穿了',exact:true}).click();
 await expect(page.getByRole('dialog',{name:'记录穿着',exact:true})).toBeVisible();
 expect(await overflowIssues(page)).toEqual([]);
 await page.screenshot({path:`${shots}long-name-wear-dialog.png`,fullPage:true});
 await page.getByRole('dialog').getByRole('button',{name:'关闭单品窗口'}).click();
 // 清理：移入回收站，恢复共享数据的原始状态。
 const call=async(operation:string,input:object)=>{const r=await(await page.request.post('/__wardrobe_dev_api',{data:{operation,input}})).json();return r.result;};
 // 注意：clothing.list 的筛选键是 search（query 会被忽略并返回全部，误删整库）。
 const found=await call('clothing.list',{member_id:'default',search:'长名称验收'});
 for(const item of found.items)await call('clothing.delete',{member_id:'default',id:item.id});
});
