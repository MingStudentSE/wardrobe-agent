import {test,expect} from '@playwright/test';
import {fileURLToPath} from 'node:url';

// 手工搭配（保存/编辑/克隆/收藏/本地拼贴/回收站恢复）与穿着（补记/更正/撤销）Web 终验，
// 并断言「保存搭配/推荐不产生穿着记录」。AI 依赖入口做能力如实声明验证。
// 搭配/穿着固定使用运动鞋与多图外套：它们不参与 20 号用例的归档/回收站流程，避免用例间级联。
const tee=fileURLToPath(new URL('../../../design/studio/assets/tee.png',import.meta.url));

test.setTimeout(240_000);

const cards=page=>page.locator('.collection .grid article');
const outfits=page=>page.locator('.saved-outfit-grid article');

function isoDate(offsetDays:number){
 const d=new Date(Date.now()+offsetDays*86400000);
 return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`;
}
function shortDate(iso:string){const [,m,d]=iso.split('-');return `${m} / ${d}`;}

async function openOutfitEditor(page:import('@playwright/test').Page,name:string){
 await outfits(page).filter({hasText:name}).first().getByRole('button',{name:'查看与调整'}).click();
 const editor=page.getByRole('dialog',{name:'调整穿搭'});
 await expect(editor).toBeVisible();
 return editor;
}

test('manual outfit: save, rename, clone, favorite, local collage, recycle bin and restore',async({page})=>{
 await page.goto('/');
 await expect(page.getByRole('heading',{name:'我的衣橱'})).toBeVisible();

 // 两件单品加入搭配并保存
 for(const name of ['Web 终验运动鞋','Web 终验多图外套']){
  await cards(page).filter({hasText:name}).first().getByRole('button',{name:'加入搭配'}).click();
 }
 const composer=page.getByRole('complementary',{name:'搭配预览'});
 await expect(composer.getByText('2 件',{exact:true})).toBeVisible();
 await composer.getByLabel('搭配名称').fill('Web 终验日常套');
 await composer.getByRole('button',{name:'✓ 保存这套穿搭'}).click();
 await expect(composer.getByRole('status')).toContainText('搭配已保存，可在穿搭库查看。');

 // 断言：保存搭配不隐式产生穿着记录
 await page.getByRole('navigation').getByRole('button',{name:'记录',exact:true}).click();
 const wears=page.locator('#wears');
 await expect(wears).toBeVisible();
 await expect(wears.getByText('Web 终验日常套')).toHaveCount(0);

 // 穿搭库：改名
 await page.getByRole('navigation').getByRole('button',{name:'穿搭',exact:true}).click();
 await expect(page.getByRole('heading',{name:'我的穿搭'})).toBeVisible();
 let editor=await openOutfitEditor(page,'Web 终验日常套');
 await editor.getByLabel('搭配名称').fill('Web 终验日常套·二');
 await editor.getByRole('button',{name:'保存搭配',exact:true}).click();
 await expect(editor.getByRole('status')).toContainText('搭配已保存。');
 await editor.getByRole('button',{name:'关闭单品窗口'}).click();
 await expect(editor).not.toBeVisible();
 await expect(outfits(page).filter({hasText:'Web 终验日常套·二'})).toHaveCount(1);

 // 克隆模板
 editor=await openOutfitEditor(page,'Web 终验日常套·二');
 await editor.getByRole('button',{name:'克隆模板'}).click();
 await expect(editor.getByLabel('搭配名称')).toHaveValue('Web 终验日常套·二 副本',{timeout:30000});
 await editor.getByRole('button',{name:'关闭单品窗口'}).click();
 await expect(outfits(page).filter({hasText:'Web 终验日常套·二 副本'})).toHaveCount(1);

 // 收藏搭配
 editor=await openOutfitEditor(page,'Web 终验日常套·二');
 await editor.getByRole('button',{name:'收藏搭配'}).click();
 await expect(editor.getByRole('status')).toContainText('搭配已更新。');
 await editor.getByRole('button',{name:'关闭单品窗口'}).click();
 await expect(outfits(page).filter({hasText:'Web 终验日常套·二'}).first()).toContainText('已收藏');

 // 本地拼贴（不依赖任何模型）：采用保存版本中的商品展示图
 editor=await openOutfitEditor(page,'Web 终验日常套·二');
 await editor.getByRole('button',{name:'本地拼贴'}).click();
 await expect(editor.locator('img.outfit-collage')).toBeVisible({timeout:30000});
 await expect(editor).toContainText('采用保存版本中的商品展示图');

 // AI 依赖入口的能力如实声明：无模型时明确提示，不伪造入口。
 // Lookbook 的外层入口 details（生成搭配展示图 / 虚拟试穿）内嵌 Flatlay/Tryon 自己的 details，需逐层展开。
 await editor.getByText('生成搭配展示图',{exact:true}).click();
 await editor.locator('details.flatlay > summary').click();
 await expect(editor.getByText('先在模型设置保存专用图片模型与系统安全凭据。')).toBeVisible();
 await editor.getByText('虚拟试穿',{exact:true}).click();
 await editor.locator('details.tryon > summary').click();
 await expect(editor.getByText('请在模型设置启用 Seedream 图片生成与虚拟试穿能力。')).toBeVisible();
 // 试穿人物导入通道在 Web 真实可用（/api/upload purpose=person）；先确认人物图片使用权
 await editor.getByText('我有权使用人物图片并已获得必要同意').click();
 const chooserPromise=page.waitForEvent('filechooser');
 await editor.getByRole('button',{name:'选择本机人物图片'}).click();
 const chooser=await chooserPromise;
 await chooser.setFiles(tee);
 await expect(editor.getByText('人物图片已私有保存到本机，尚未发送。')).toBeVisible({timeout:30000});
 await expect(editor.getByRole('combobox',{name:'试穿人物'})).toContainText('本人');
 await editor.getByRole('button',{name:'关闭单品窗口'}).click();
 await expect(editor).not.toBeVisible();

 // 回收站：移入 → 恢复
 editor=await openOutfitEditor(page,'Web 终验日常套·二 副本');
 await editor.getByRole('button',{name:'移入搭配回收站'}).click();
 await editor.getByRole('button',{name:'确认移入回收站'}).click();
 await expect(editor.getByRole('button',{name:'恢复搭配'})).toBeVisible({timeout:30000});
 await editor.getByRole('button',{name:'关闭单品窗口'}).click();
 await page.getByRole('button',{name:'回收站',exact:true}).click();
 await expect(outfits(page).filter({hasText:'Web 终验日常套·二 副本'})).toHaveCount(1);
 editor=await openOutfitEditor(page,'Web 终验日常套·二 副本');
 await editor.getByRole('button',{name:'恢复搭配'}).click();
 await expect(editor.getByRole('status')).toContainText('搭配已更新。');
 await editor.getByRole('button',{name:'关闭单品窗口'}).click();
 await page.getByRole('button',{name:'全部穿搭'}).click();
 await expect(outfits(page).filter({hasText:'Web 终验日常套·二 副本'})).toHaveCount(1);

 // 重载持久：搭配库来自真实持久查询（重载后回到默认衣橱视图，需重新进入穿搭库）
 await page.reload();
 await page.getByRole('navigation').getByRole('button',{name:'穿搭',exact:true}).click();
 await expect(outfits(page).filter({hasText:'Web 终验日常套·二'})).toHaveCount(2);
});

test('wear journal: backfill, correct, revoke with full history and no implicit records',async({page})=>{
 await page.goto('/');
 await page.getByRole('navigation').getByRole('button',{name:'记录',exact:true}).click();
 const wears=page.locator('#wears');
 await expect(wears).toBeVisible();
 const wornMetric=()=>wears.locator('.journal-metrics article').first().locator('strong');
 // 等待加载完成再读基线：10 号用例已记录一次穿着，时间线非空
 await expect(wears.locator('.journal-timeline article').first()).toBeVisible({timeout:30000});
 const baseline=parseInt((await wornMetric().innerText()).trim(),10);
 expect(baseline).toBeGreaterThanOrEqual(1);
 // 前置：保存过的搭配没有产生穿着（时间线无搭配名）
 await expect(wears.getByText('Web 终验日常套')).toHaveCount(0);

 // 补记：日期填昨天（显式补记，不依赖浏览器时钟语义）
 await wears.getByRole('button',{name:'＋ 记录穿着'}).click();
 const form=page.getByRole('dialog',{name:'记录穿着'});
 await form.getByLabel('穿着日期').fill(isoDate(-1));
 await form.getByRole('button',{name:'选择 Web 终验运动鞋'}).click();
 await form.getByRole('button',{name:'确认实际穿过',exact:true}).click();
 await expect(wears.locator('p.feedback.success')).toContainText('实际穿着已记录。',{timeout:30000});
 await expect(wornMetric()).toContainText(`${baseline+1}`);
 const entry=wears.locator('.journal-timeline article').filter({hasText:'Web 终验运动鞋'}).first();
 await expect(entry).toBeVisible();
 await expect(entry.locator('.journal-date')).toContainText(shortDate(isoDate(-1)));

 // 更正：修改备注，旧事件保留并可查历史
 await entry.getByRole('button',{name:'更正记录'}).click();
 const correct=page.getByRole('dialog',{name:'更正穿着记录'});
 await correct.getByLabel('穿着备注').fill('终验更正备注');
 await correct.getByRole('button',{name:'确认更正'}).click();
 await expect(wears.locator('p.feedback.success')).toContainText('实际穿着已更正，旧事件保留。',{timeout:30000});
 await entry.getByRole('button',{name:'修改历史'}).click();
 const history=page.getByRole('dialog',{name:'穿着修改历史'});
 await expect(history.getByLabel('事件追加历史')).toContainText('记录');
 await expect(history.getByLabel('事件追加历史')).toContainText('更正');
 await history.getByRole('button',{name:'关闭单品窗口'}).click();

 // 撤销：需要原因，撤销后不计次数，历史保留
 await entry.getByRole('button',{name:'撤销记录'}).click();
 const revoke=page.getByRole('dialog',{name:'撤销穿着记录'});
 await revoke.getByLabel('撤销原因').fill('终验误记');
 await revoke.getByRole('button',{name:'确认撤销'}).click();
 await expect(wears.locator('p.feedback.success')).toContainText('实际穿着已撤销，历史保留。',{timeout:30000});
 await expect(wornMetric()).toContainText(`${baseline}`);
 await expect(wears.locator('.journal-timeline article').filter({hasText:'已撤销'})).toHaveCount(0);
 await wears.getByLabel('显示已撤销').check();
 await expect(wears.locator('.journal-timeline article').filter({hasText:'已撤销'}).first()).toBeVisible();
 await wears.getByLabel('显示已撤销').uncheck();

 // 洞察：真实统计渲染（有效事件不含已撤销）
 await page.getByRole('button',{name:'衣橱洞察',exact:true}).click();
 const insights=page.locator('#insights');
 await expect(insights.getByRole('heading',{name:'实际穿着频率'})).toBeVisible({timeout:30000});
 await expect(insights.locator('.journal-metrics article').filter({hasText:'衣橱单品'}).locator('strong')).not.toContainText('0');
 await expect(insights.locator('.journal-metrics article').filter({hasText:'实际穿着'}).locator('strong')).toContainText(`${baseline}`);
});
