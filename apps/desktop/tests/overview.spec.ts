import {test,expect} from '@playwright/test';
// 衣橱概览验收：计数覆盖全部库存（在用/归档/回收站），手动天气经 localStorage
// 传入 AI 推荐入口。共享 dev server 上库存会被其他规格增删：数量以真实读取为准。
test('overview counts the whole wardrobe and passes manual weather to AI entry',async({page})=>{
 await page.goto('/');
 const call=async(operation:string,input:object={})=>{const b=await(await page.request.post('/__wardrobe_dev_api',{data:{operation,input:{member_id:'default',...input}}})).json();if(b.error)throw new Error(b.error.message);return b.result;};
 const total=await (async()=>{let cursor:string|null=null,n=0;do{const r=await call('clothing.list',{status:'active',limit:100,...(cursor?{cursor}:{})});n+=r.items.length;cursor=r.next_cursor;}while(cursor);return n;})();
 const outfitCount=(await call('outfit.list',{status:'active'})).items.length;
 await expect(page.locator('.wardrobe-counts')).toHaveText(`${total} 件单品 · ${outfitCount} 套穿搭`);
 await page.getByLabel('搜索衣物',{exact:true}).fill('不存在');
 await page.getByRole('button',{name:'搜索',exact:true}).click();
 await expect(page.locator('.wardrobe-counts')).toHaveText(`${total} 件单品 · ${outfitCount} 套穿搭`);
 await page.getByRole('button',{name:'手填',exact:true}).click();
 await page.getByLabel('地点',{exact:true}).fill('上海');
 await page.getByLabel('气温（°C）',{exact:true}).fill('22');
 await page.getByRole('button',{name:'保存天气',exact:true}).click();
 await expect(page.getByRole('region',{name:'地点与天气'})).toContainText('22°C · 手动填写');
 await page.getByRole('button',{name:'AI 推荐',exact:true}).click();
 const rec=page.locator('#recommendations');
 // AI 入口预选 AI 模式，并带入手填天气（打开“天气与高级设置”读取）。
 await expect(rec.getByRole('button',{name:'AI 推荐',exact:true})).toHaveAttribute('aria-pressed','true');
 await rec.locator('summary').filter({hasText:'天气与高级设置'}).click();
 await expect(rec.getByLabel('手动位置')).toHaveValue('上海');
 await expect(rec.getByRole('spinbutton',{name:'手动温度 °C',exact:true})).toHaveValue('22');
});
