import {test,expect} from '@playwright/test';

// 洗护（阈值/手动待洗/干洗/更正/撤销）、离线规则推荐（硬约束/换一套/保存不记穿着）与
// 洞察阈值设置的 Web 终验：全部经真实服务与会话 API，不依赖任何模型。
// 注意：新 UI 的洗护录入表单在「＋ 记录洗护」对话框内；推荐固定项使用多图外套（不参与其它用例的状态流转）。
test.setTimeout(240_000);

test('care loop: threshold, manual dirty, dry clean, correction and revocation',async({page})=>{
 await page.goto('/');
 await page.getByRole('navigation').getByRole('button',{name:'记录',exact:true}).click();
 await page.getByRole('button',{name:'洗护记录',exact:true}).click();
 const care=page.locator('#care');
 await expect(care).toBeVisible();

 // 打开录入对话框：阈值配置真实持久（expected_revision 乐观锁）。
 // 40 号用例给运动鞋记过一次穿着，这里不断言具体次数，只断言状态行出现。
 await care.getByRole('button',{name:'＋ 记录洗护'}).click();
 let form=page.getByRole('dialog',{name:'记录洗护'});
 await form.getByLabel('选择洗护单品').selectOption({label:'Web 终验运动鞋'});
 await expect(form).toContainText(/最近有效清洗后 \d+ 次/);
 await form.getByLabel('洗护阈值',{exact:true}).fill('2');
 await form.getByRole('button',{name:'保存阈值',exact:true}).click();
 await expect(care.getByText('阈值已保存。')).toBeVisible({timeout:30000});

 // 手动标记待洗 → 提交后对话框关闭，待洗列表出现该单品
 await form.getByLabel('洗护类型').selectOption('mark_dirty');
 await form.getByLabel('洗护日期').fill('2020-01-01');
 await form.getByRole('button',{name:'确认记录洗护',exact:true}).click();
 await expect(page.getByRole('dialog',{name:'记录洗护'})).not.toBeVisible({timeout:30000});
 await expect(care.getByText('洗护已保存，状态按有效时间线重算。')).toBeVisible();
 await expect(care.locator('.care-overview')).toContainText('Web 终验运动鞋');
 await expect(care.locator('.care-overview')).toContainText('手动标记');
 // 重新打开对话框可读到重算后的可用性（自动推荐排除）
 await care.getByRole('button',{name:'＋ 记录洗护'}).click();
 form=page.getByRole('dialog',{name:'记录洗护'});
 await form.getByLabel('选择洗护单品').selectOption({label:'Web 终验运动鞋'});
 await expect(form).toContainText('自动推荐排除');
 await expect(form).toContainText('手动待洗');
 await form.getByRole('button',{name:'关闭单品窗口'}).click();
 await expect(form).not.toBeVisible();

 // 干洗解除待洗 → 待洗列表清空
 await care.getByRole('button',{name:'＋ 记录洗护'}).click();
 form=page.getByRole('dialog',{name:'记录洗护'});
 await form.getByLabel('选择洗护单品').selectOption({label:'Web 终验运动鞋'});
 await form.getByLabel('洗护类型').selectOption('dry_clean');
 await form.getByLabel('洗护日期').fill('2020-01-02');
 await form.getByRole('button',{name:'确认记录洗护',exact:true}).click();
 await expect(care).toContainText('没有待洗单品',{timeout:30000});
 await care.getByRole('button',{name:'＋ 记录洗护'}).click();
 form=page.getByRole('dialog',{name:'记录洗护'});
 await form.getByLabel('选择洗护单品').selectOption({label:'Web 终验运动鞋'});
 await expect(form).toContainText('可推荐');
 await form.getByRole('button',{name:'关闭单品窗口'}).click();
 await expect(care).toContainText('衣物都准备好了');

 // 更正干洗日期到待洗之前 → 手动待洗重新生效（状态按有效时间线重算）
 const clean=care.locator('.care-events li').filter({hasText:'Web 终验运动鞋 · 干洗'});
 await clean.getByRole('button',{name:'更正洗护',exact:true}).click();
 const correctForm=page.getByRole('dialog',{name:'更正洗护记录'});
 await correctForm.getByLabel('洗护日期').fill('2019-12-31');
 await correctForm.getByRole('button',{name:'确认更正洗护',exact:true}).click();
 await expect(care.getByText('洗护已保存，状态按有效时间线重算。')).toBeVisible({timeout:30000});
 await expect(care.locator('.care-overview')).toContainText('手动标记',{timeout:30000});

 // 撤销干洗 → 历史保留，修订可见
 await clean.getByRole('button',{name:'撤销洗护',exact:true}).click();
 const revoke=page.getByRole('dialog',{name:'撤销洗护记录'});
 await revoke.getByLabel('洗护撤销原因').fill('终验误记');
 await revoke.getByRole('button',{name:'确认撤销洗护'}).click();
 await expect(care.getByText('已撤销，历史保留并重算。')).toBeVisible({timeout:30000});
 await care.locator('.care-events li').filter({hasText:'已撤销'}).first().getByRole('button',{name:'查看洗护修订'}).click();
 const history=page.getByRole('dialog',{name:'洗护修改历史'});
 await expect(history.getByLabel('洗护修订历史')).toContainText('修订 3');
 await history.getByRole('button',{name:'关闭单品窗口'}).click();

 // 重载持久：洗护历史与状态来自真实 SQLite
 await page.reload();
 await page.getByRole('navigation').getByRole('button',{name:'记录',exact:true}).click();
 await page.getByRole('button',{name:'洗护记录',exact:true}).click();
 await expect(page.locator('#care')).toContainText('手动标记',{timeout:30000});
 await expect(page.locator('#care').locator('.care-events li').filter({hasText:'已撤销'})).toHaveCount(1);
});

test('offline rule recommendations honor fixed constraints and never record wears on save',async({page})=>{
 await page.goto('/');
 await page.getByRole('navigation').getByRole('button',{name:'穿搭',exact:true}).click();
 await page.getByRole('button',{name:'帮我搭配',exact:true}).click();
 const recs=page.locator('#recommendations');
 await expect(recs.getByRole('heading',{name:'今天，想怎么穿？'})).toBeVisible();
 // 默认离线规则（不依赖模型）
 await expect(recs.getByRole('button',{name:'离线搭配'})).toHaveAttribute('aria-pressed','true');

 // 硬约束：固定多图外套（库存中有多件上装可形成替代组合）
 await recs.getByText('指定想穿的单品').click();
 await recs.getByRole('checkbox',{name:'固定 Web 终验多图外套'}).check();
 await expect(recs).toContainText('已固定 1 件');

 await recs.getByRole('button',{name:'为我推荐搭配'}).click();
 await expect(recs.getByText(/为你找到 [1-9]\d* 套搭配/)).toBeVisible({timeout:30000});
 const proposals=recs.locator('.recommendation-proposal');
 const count=await proposals.count();
 expect(count,'硬约束下应至少返回一套搭配').toBeGreaterThanOrEqual(1);
 for(let index=0;index<count;index++){
  await expect(proposals.nth(index)).toContainText('Web 终验多图外套',`第 ${index+1} 套必须含固定单品`);
 }

 // 换一套：约束在替换后仍然成立
 await proposals.first().getByRole('button',{name:'换一套'}).click();
 await expect(recs.getByText(/为你找到 [1-9]\d* 套搭配/)).toBeVisible({timeout:30000});
 const replaced=recs.locator('.recommendation-proposal');
 const replacedCount=await replaced.count();
 expect(replacedCount).toBeGreaterThanOrEqual(1);
 for(let index=0;index<replacedCount;index++){
  await expect(replaced.nth(index)).toContainText('Web 终验多图外套');
 }
 await expect(recs.getByRole('alert')).toHaveCount(0);

 // 保存为搭配：只写搭配模板，不产生穿着
 await replaced.first().getByLabel('搭配保存名称').fill('Web 终验推荐套');
 await replaced.first().getByRole('button',{name:'保存为搭配'}).click();
 await expect(recs.getByRole('status')).toContainText('已保存到穿搭库。',{timeout:30000});
 await page.getByRole('navigation').getByRole('button',{name:'记录',exact:true}).click();
 const wears=page.locator('#wears');
 await expect(wears).toBeVisible();
 await expect(wears.getByText('Web 终验推荐套')).toHaveCount(0);
});

test('insight threshold configuration persists and recomputes',async({page})=>{
 await page.goto('/');
 await page.getByRole('navigation').getByRole('button',{name:'记录',exact:true}).click();
 await page.getByRole('button',{name:'衣橱洞察',exact:true}).click();
 const insights=page.locator('#insights');
 await expect(insights.getByRole('heading',{name:'实际穿着频率'})).toBeVisible({timeout:30000});
 // 分布与组合统计渲染
 await expect(insights.getByRole('heading',{name:'类别分布'})).toBeVisible();
 await expect(insights.getByRole('heading',{name:'颜色标签分布'})).toBeVisible();
 // 久未穿阈值：写入真实偏好并可恢复默认
 await insights.getByText('统计设置与口径').click();
 await insights.getByLabel('久未穿天数').fill('30');
 await insights.getByRole('button',{name:'保存久未穿阈值'}).click();
 await expect(insights.getByText('久未穿阈值已保存，重启后继续使用。')).toBeVisible({timeout:30000});
 await insights.getByRole('button',{name:'恢复默认 90 天'}).click();
 await expect(insights.getByText('已恢复默认 90 天。')).toBeVisible({timeout:30000});
 // 重载后洞察照常重算（真实持久设置）
 await page.reload();
 await page.getByRole('navigation').getByRole('button',{name:'记录',exact:true}).click();
 await page.getByRole('button',{name:'衣橱洞察',exact:true}).click();
 await expect(page.locator('#insights').getByRole('heading',{name:'实际穿着频率'})).toBeVisible({timeout:30000});
});
