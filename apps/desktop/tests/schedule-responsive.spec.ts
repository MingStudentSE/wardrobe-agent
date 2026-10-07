import {test,expect} from '@playwright/test';
// 本测试只验证浏览器预览可达的界面事实：真实 CLI 后端（/__wardrobe_dev_api 桥接 wardrobe-cli），
// 键盘与可见控件完成“次日搭配推荐”计划创建、修改、暂停、重载后的持久性，
// 并在七档宽度下检查无横向溢出、关键按钮完整可见。原生通知、后台运行与登录自启不属于本测试（浏览器预览无法验证）。
const WIDTHS=[320,360,390,768,1024,1280,1440];
const NAME='次日推荐响应式计划';
test.setTimeout(240000);
test('七档宽度下用键盘与可见控件创建次日搭配推荐计划，修改、暂停、重载后目标日持久，关键按钮不溢出不裁剪',async({page})=>{
 const EVIDENCE=test.info().outputPath();
 await page.setViewportSize({width:320,height:1000});
 await page.goto('/');const choice=page.getByRole('combobox',{name:'当前成员',exact:true});await expect(choice).toBeEnabled({timeout:30000});
 const member=await choice.inputValue();expect(member.length).toBeGreaterThan(0);
 await page.getByText('定时计划与通知',{exact:true}).click();
 // 以最窄宽度 320 通过可见控件录入，并用回车键提交表单，证明关键流程不依赖悬停、拖拽或桌面宽度。
 await page.getByLabel('计划名称',{exact:true}).fill(NAME);
 await page.getByRole('combobox',{name:'计划内容',exact:true}).selectOption('recommendation');
 await page.getByRole('combobox',{name:'准备哪一天',exact:true}).selectOption({label:'次日（前夜准备）'});
 await page.getByLabel('本地执行时间',{exact:true}).fill('21:30');
 await page.getByLabel('计划时区',{exact:true}).fill('Asia/Shanghai');
 await page.getByLabel('计划名称',{exact:true}).press('Enter');
 await expect(page.getByRole('status').filter({hasText:'计划已创建'})).toBeVisible();
 const row=page.getByRole('listitem').filter({hasText:NAME});await expect(row).toContainText('次日搭配推荐');await expect(row).toContainText('启用');
 await page.screenshot({path:`${EVIDENCE}/schedule-created-320.png`,fullPage:true});
 // 修改：表单回填“准备哪一天”仍为次日（target_day_offset=1），仅改时间后保存。
 await row.getByRole('button',{name:'修改计划',exact:true}).click();
 await expect(page.getByRole('heading',{name:'修改未来计划'})).toBeVisible();
 await expect(page.getByRole('combobox',{name:'准备哪一天',exact:true})).toHaveValue('1');
 await page.getByLabel('本地执行时间',{exact:true}).fill('22:15');
 await page.getByRole('button',{name:'保存计划修改',exact:true}).click();
 await expect(page.getByRole('status').filter({hasText:'计划已更新'})).toBeVisible();
 await page.screenshot({path:`${EVIDENCE}/schedule-edited-320.png`,fullPage:true});
 // 暂停：状态文案与开关按钮都要翻转，避免“暂停计划”按钮自身文案造成的假通过。
 await row.getByRole('button',{name:'暂停计划',exact:true}).click();
 await expect(page.getByRole('status').filter({hasText:'计划已暂停'})).toBeVisible();
 await expect(row).toContainText('暂停');await expect(row.getByRole('button',{name:'启用计划',exact:true})).toBeVisible();
 // 重载后持久：次日搭配推荐（target_day_offset=1）、暂停状态、修改后的执行时间都在。
 await page.reload();await expect(choice).toBeEnabled({timeout:30000});await page.getByText('定时计划与通知',{exact:true}).click();
 const persisted=page.getByRole('listitem').filter({hasText:NAME});
 await expect(persisted).toContainText('次日搭配推荐');await expect(persisted).toContainText('暂停');await expect(persisted).toContainText('22:15');
 // 经真实 CLI 后端确认持久化事实（开发桥即 wardrobe-cli --trusted-local，非 mock、非私有库写入）。
 const response=await page.request.post('/__wardrobe_dev_api',{data:{operation:'schedule.list',input:{member_id:member}}});
 const body=await response.json();expect(body.error).toBeUndefined();
 const plan=body.result.plans.find((p:{name:string})=>p.name===NAME);
 expect(plan,'计划应经 CLI 持久化').toBeTruthy();
 expect(plan.target_day_offset).toBe(1);expect(plan.enabled).toBe(false);expect(plan.local_time).toBe('22:15');expect(plan.timezone).toBe('Asia/Shanghai');
 // 执行记录此刻应为空：计划评估游标自创建时刻起，次日目标日期要等首个到期时刻过后才生成执行记录。
 await persisted.getByRole('button',{name:'查看执行记录',exact:true}).click();
 await expect(page.getByLabel('计划执行记录')).toContainText('还没有执行记录');
 await page.screenshot({path:`${EVIDENCE}/schedule-reloaded-320.png`,fullPage:true});
 // 七档宽度：无横向溢出，且创建、修改、暂停开关、执行记录、到期检查按钮均完整落在视口内。
 for(const width of WIDTHS){
  await page.setViewportSize({width,height:1000});
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth),`宽度 ${width} 不应出现横向溢出`).toBeTruthy();
  for(const label of ['创建定时计划','修改计划','启用计划','查看执行记录','检查到期计划']){
   const button=page.getByRole('button',{name:label,exact:true});
   await expect(button,`宽度 ${width} 的“${label}”应可见`).toBeVisible();
   const box=await button.boundingBox();
   expect(box,`宽度 ${width} 的“${label}”应有边界`).not.toBeNull();
   const innerWidth=await page.evaluate(()=>window.innerWidth);
   expect(box!.x,`宽度 ${width} 的“${label}”左缘不应越界`).toBeGreaterThanOrEqual(0);
   expect(box!.x+box!.width,`宽度 ${width} 的“${label}”右缘不应被裁剪`).toBeLessThanOrEqual(innerWidth);
  }
  await page.screenshot({path:`${EVIDENCE}/schedule-responsive-${width}.png`,fullPage:true});
 }
});
