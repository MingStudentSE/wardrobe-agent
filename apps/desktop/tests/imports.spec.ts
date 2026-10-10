import {test,expect} from '@playwright/test';
import {fileURLToPath} from 'node:url';
// 批量录入验收（新 UI）：入口为 ＋ 新增衣物 → 批量录入 方式页签；浏览器经隐藏
// file input 提供 1–100 个文件。行为断言（持久批次、独立失败、归并、草稿名称
// 持久、入库幂等、取消队列跨重启、单文件重试）保持不变。
const source=fileURLToPath(new URL('../../../crates/wardrobe-core/tests/fixtures/generated-colors.png',import.meta.url));
const invalid=fileURLToPath(new URL('../../../crates/wardrobe-core/tests/fixtures/over-limit-header.png',import.meta.url));
async function openBatch(page:any){
 await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();
 await page.getByRole('button',{name:'批量录入',exact:true}).click();
 const region=page.getByRole('region',{name:'批量录入',exact:true});
 await expect(region.getByRole('button',{name:'选择批量图片'})).toBeVisible();
 return region;
}
async function chooseFiles(page:any,region:any,files:string[]){
 await region.locator('input[type=file]').setInputFiles(files);
}
test.beforeEach(async({page},info)=>{
 await page.goto('/');
 await page.getByText('当前衣橱',{exact:true}).click();
 await page.getByText('管理成员档案',{exact:true}).click();
 await page.getByLabel('新成员名称').fill(`批量验收 ${info.title.slice(0,20)}`);
 await page.getByRole('button',{name:'创建成员',exact:true}).click();
 await expect(page.getByRole('article')).toHaveCount(0);
 await expect(page.getByRole('combobox',{name:/^当前成员/})).toBeEnabled();
});
test.afterEach(async({page})=>{
 // 测试可能停在新增衣物弹窗（模态遮挡侧栏）：先关闭再恢复成员选择。
 const leftover=page.getByRole('dialog');
 if(await leftover.count())await leftover.getByRole('button',{name:'关闭单品窗口'}).click({timeout:3000}).catch(()=>{});
 const choice=page.getByRole('combobox',{name:/^当前成员/});
 if(!(await choice.isVisible()))await page.getByText('当前衣橱',{exact:true}).click();
 await expect(choice).toBeEnabled();
 await choice.selectOption({label:'我'});
 await expect(page.getByRole('button',{name:'＋ 新增衣物',exact:true})).toBeEnabled();
});
test('durable batch drafts merge save reload and show corrupt file independently',async({page})=>{
 await page.goto('/');
 let region=await openBatch(page);
 await chooseFiles(page,region,[source,source,invalid]);
 await expect(region.getByText('共 3 个文件')).toBeVisible();
 await page.reload();
 region=await openBatch(page);
 await expect(region.getByText('共 3 个文件')).toBeVisible();
 await region.getByRole('button',{name:'处理待办图片'}).click();
 await expect(region.getByRole('img',{name:'草稿图片'})).toHaveCount(2);
 await expect(region.getByRole('alert')).toContainText('60 MP');
 await region.getByRole('checkbox',{name:'归并此草稿'}).nth(0).check();
 await region.getByRole('checkbox',{name:'归并此草稿'}).nth(1).check();
 await region.getByRole('button',{name:'归并选中草稿'}).click();
 await expect(region.getByRole('textbox',{name:'草稿名称'})).toHaveCount(2);
 await region.getByRole('textbox',{name:'草稿名称'}).first().fill('批量双图上衣');
 await region.getByRole('button',{name:'保存草稿'}).first().click();
 await page.reload();
 region=await openBatch(page);
 await expect(region.getByRole('textbox',{name:'草稿名称'}).first()).toHaveValue('批量双图上衣');
 await region.getByRole('button',{name:'入库此草稿'}).first().click();
 await expect(region.getByText('此草稿已入库',{exact:true})).toBeVisible();
 await page.reload();
 region=await openBatch(page);
 await expect(region.getByText('此草稿已入库',{exact:true})).toBeVisible();
 for(const width of [320,375,414,768,1024,1280,1440]){await page.setViewportSize({width,height:900});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBeTruthy();await region.getByRole('button',{name:'选择批量图片'}).scrollIntoViewIfNeeded();await expect(region.getByRole('button',{name:'选择批量图片'})).toBeVisible();await region.screenshot({path:`/tmp/wardrobe-batch-${width}.png`});}
});
test('cancelled perfile queue survives reload and explicit retry resumes only that file',async({page})=>{
 await page.goto('/');
 let region=await openBatch(page);
 await chooseFiles(page,region,[source,source]);
 await expect(region.getByText('共 2 个文件')).toBeVisible();
 await region.getByRole('button',{name:'取消此图片'}).first().click();
 await expect(region.getByText('已取消',{exact:true})).toBeVisible();
 await page.reload();
 region=await openBatch(page);
 await expect(region.getByText('已取消',{exact:true})).toBeVisible();
 await region.getByRole('button',{name:'处理待办图片'}).click();
 await expect(region.getByRole('img',{name:'草稿图片'})).toHaveCount(1);
 await region.getByRole('button',{name:'重试此图片'}).click();
 await region.getByRole('button',{name:'处理待办图片'}).click();
 await expect(region.getByRole('img',{name:'草稿图片'})).toHaveCount(2);
 await expect(region.getByRole('button',{name:'入库此草稿'}).first()).toBeEnabled();
});
