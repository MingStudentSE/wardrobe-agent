import {test,expect} from '@playwright/test';
import {fileURLToPath} from 'node:url';

// 库存/分类工作区（Web 终验）：列表筛选、收藏、排序、分类管理、归档、回收站与恢复、重载持久。
// 夹具图片不入 git（用户私人资产），由执行环境从主工作区拷贝。
const jacket=fileURLToPath(new URL('../../../design/studio/assets/jacket.png',import.meta.url));
const tee=fileURLToPath(new URL('../../../design/studio/assets/tee.png',import.meta.url));
const shoes=fileURLToPath(new URL('../../../design/studio/assets/shoes.png',import.meta.url));

test.setTimeout(180_000);

const cards=page=>page.locator('.collection .grid article');

async function createItem(page:import('@playwright/test').Page,options:{name:string;categoryLabel?:string;photo:string;favorite?:boolean}){
 await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();
 const dialog=page.getByRole('dialog');
 const region=dialog.getByRole('region',{name:'录入单品'});
 await region.locator('input[type=file]').setInputFiles(options.photo);
 await expect(dialog.getByText('照片已准备好')).toBeVisible({timeout:30000});
 await region.getByLabel('名称',{exact:true}).fill(options.name);
 if(options.categoryLabel)await region.getByRole('combobox',{name:'类别'}).selectOption({label:options.categoryLabel});
 if(options.favorite)await dialog.locator('.create-footer').getByRole('button',{name:'加入收藏'}).click();
 await dialog.getByRole('button',{name:'添加到衣橱',exact:true}).click();
 await expect(dialog).not.toBeVisible();
}

async function search(page:import('@playwright/test').Page,keyword:string){
 await page.getByLabel('搜索衣物',{exact:true}).first().fill(keyword);
 await page.getByRole('button',{name:'搜索',exact:true}).click();
}

async function applyStatusFilter(page:import('@playwright/test').Page,status:string){
 await page.getByRole('button',{name:'筛选',exact:true}).click();
 const filterDialog=page.getByRole('dialog',{name:'筛选衣物'});
 await filterDialog.getByRole('button',{name:status,exact:true}).click();
 await filterDialog.getByRole('button',{name:'应用筛选'}).click();
 await expect(filterDialog).not.toBeVisible();
}

test('inventory list: filter by search/category/favorite and sort, with reload persistence',async({page})=>{
 await page.goto('/');
 await expect(page.getByRole('heading',{name:'我的衣橱'})).toBeVisible();

 // 四件带真实照片的单品（其中毛衣收藏）
 await createItem(page,{name:'Web 终验毛衣',photo:tee,favorite:true});
 await createItem(page,{name:'Web 终验长裤',categoryLabel:'下装',photo:shoes});
 await createItem(page,{name:'Web 终验运动鞋',categoryLabel:'鞋',photo:shoes});
 await createItem(page,{name:'Web 终验多图外套',categoryLabel:'外套',photo:jacket});

 // 搜索过滤：命中 4 件，不含此前批次录入的单品
 await search(page,'Web 终验');
 await expect(cards(page)).toHaveCount(4);

 // 收藏筛选：仅已收藏的毛衣
 await page.getByRole('button',{name:'筛选',exact:true}).click();
 const filterDialog=page.getByRole('dialog',{name:'筛选衣物'});
 await filterDialog.getByRole('button',{name:'已收藏',exact:true}).click();
 await filterDialog.getByRole('button',{name:'应用筛选'}).click();
 await expect(filterDialog).not.toBeVisible();
 await expect(page.getByRole('button',{name:'移除收藏筛选'})).toBeVisible();
 await expect(cards(page)).toHaveCount(1);
 await expect(cards(page).first()).toContainText('Web 终验毛衣');
 await page.getByRole('button',{name:'移除收藏筛选'}).click();
 await expect(cards(page)).toHaveCount(4);

 // 清空搜索后做品类与总量断言（搜索过滤与品类过滤是叠加的）。
 // 总量=6：本用例 4 件 + 核心流程用例的夹克与其批量入库单品。
 await page.getByLabel('搜索衣物',{exact:true}).first().fill('');
 await page.getByRole('button',{name:'搜索',exact:true}).click();
 await expect(cards(page)).toHaveCount(6);
 await page.getByRole('button',{name:'上装',exact:true}).click();
 await expect(cards(page)).toHaveCount(2);
 await page.getByRole('button',{name:'全部',exact:true}).click();
 await expect(cards(page)).toHaveCount(6);

 // 排序：录入顺序 → 名称（此时品类全部=5 件）
 await page.getByRole('button',{name:'排序：录入顺序'}).click();
 const menu=page.getByRole('menu',{name:'排序方式'});
 await menu.getByRole('menuitemradio',{name:'名称'}).click();
 await expect(page.getByRole('button',{name:'排序：名称'})).toBeVisible();

 // 重载持久：筛选与数据是真实持久查询，刷新后重新搜索仍命中
 await page.reload();
 await search(page,'Web 终验');
 await expect(cards(page)).toHaveCount(4);
 await expect(cards(page).filter({hasText:'Web 终验毛衣'}).locator('img')).toBeVisible();

 // 键盘可达：跳到内容（与核心流程用例同型：reload 且应用渲染完成后，首个 Tab 聚焦 skip-link）
 await page.reload();
 await expect(page.getByRole('heading',{name:'我的衣橱'})).toBeVisible();
 await page.keyboard.press('Tab');
 await expect(page.locator('.skip-link')).toBeFocused();
});

test('archive and recycle bin round-trips with browser-driven restore',async({page})=>{
 await page.goto('/');
 await expect(page.getByRole('heading',{name:'我的衣橱'})).toBeVisible();

 // 归档毛衣：详情 → 编辑 → 历史与管理 → 归档单品。
 // 应用行为：归档/删除等操作后编辑窗口保持打开（关闭守卫），需手动关闭。
 await search(page,'Web 终验毛衣');
 await cards(page).first().getByRole('button',{name:'查看详情'}).click();
 const detail=page.getByRole('dialog',{name:'单品详情'});
 await expect(detail).toBeVisible();
 await detail.getByRole('button',{name:'编辑衣物'}).click();
 let editor=page.getByRole('dialog',{name:'编辑衣物'});
 await editor.getByRole('button',{name:'历史与管理'}).click();
 await editor.getByRole('button',{name:'归档单品'}).click();
 await expect(editor.getByText('已更新本地单品。')).toBeVisible({timeout:30000});
 await editor.getByRole('button',{name:'关闭单品窗口'}).click();
 await expect(editor).not.toBeVisible();
 await expect(cards(page)).toHaveCount(0);

 // 归档筛选可见，恢复后在库
 await applyStatusFilter(page,'归档');
 await expect(cards(page)).toHaveCount(1);
 await cards(page).first().getByRole('button',{name:'查看详情'}).click();
 await page.getByRole('dialog',{name:'单品详情'}).getByRole('button',{name:'编辑衣物'}).click();
 editor=page.getByRole('dialog',{name:'编辑衣物'});
 await editor.getByRole('button',{name:'历史与管理'}).click();
 await editor.getByRole('button',{name:'取消归档'}).click();
 await expect(editor.getByText('已更新本地单品。')).toBeVisible({timeout:30000});
 await editor.getByRole('button',{name:'关闭单品窗口'}).click();
 await expect(cards(page)).toHaveCount(0);
 await page.getByRole('button',{name:'移除库存筛选'}).click();
 await search(page,'Web 终验毛衣');
 await expect(cards(page)).toHaveCount(1);

 // 回收站：长裤移入 → 回收站筛选可见 → 恢复
 await search(page,'Web 终验长裤');
 await cards(page).first().getByRole('button',{name:'查看详情'}).click();
 await page.getByRole('dialog',{name:'单品详情'}).getByRole('button',{name:'编辑衣物'}).click();
 editor=page.getByRole('dialog',{name:'编辑衣物'});
 await editor.getByRole('button',{name:'历史与管理'}).click();
 await editor.getByRole('button',{name:'移入回收站'}).click();
 await editor.getByRole('button',{name:'确认移入'}).click();
 await expect(editor.getByText('已更新本地单品。')).toBeVisible({timeout:30000});
 await editor.getByRole('button',{name:'关闭单品窗口'}).click();
 await expect(cards(page)).toHaveCount(0);
 await applyStatusFilter(page,'回收站');
 await expect(cards(page)).toHaveCount(1);
 await cards(page).first().getByRole('button',{name:'查看详情'}).click();
 await page.getByRole('dialog',{name:'单品详情'}).getByRole('button',{name:'恢复与历史'}).click();
 editor=page.getByRole('dialog',{name:'编辑衣物'});
 await expect(editor.getByText('此单品已移入回收站，恢复后可编辑。')).toBeVisible();
 await editor.getByRole('button',{name:'恢复单品'}).click();
 await expect(editor.getByText('已更新本地单品。')).toBeVisible({timeout:30000});
 await editor.getByRole('button',{name:'关闭单品窗口'}).click();
 await page.getByRole('button',{name:'移除库存筛选'}).click();
 await search(page,'Web 终验长裤');
 await expect(cards(page)).toHaveCount(1);
});

test('category manager creates a custom category that becomes a filter tab',async({page})=>{
 await page.goto('/');
 await expect(page.getByRole('heading',{name:'我的衣橱'})).toBeVisible();
 await page.getByRole('button',{name:'＋ 管理分类'}).click();
 const manager=page.getByRole('dialog',{name:'管理分类'});
 await manager.getByLabel('新增分类').fill('家居服');
 await manager.getByRole('button',{name:'添加',exact:true}).click();
 await expect(manager.locator('ul')).toContainText('家居服');
 await manager.getByRole('button',{name:'完成'}).click();
 await expect(manager).not.toBeVisible();
 // 新分类出现在品类筛选中且为空集
 await page.getByRole('button',{name:'家居服',exact:true}).click();
 await expect(page.getByText('没有符合条件的单品')).toBeVisible();
 // 重载后自定义分类仍在（真实持久分类表）
 await page.reload();
 await expect(page.getByRole('button',{name:'家居服',exact:true})).toBeVisible();
});

test('collection stays usable at 375px without page-level overflow',async({page})=>{
 await page.goto('/');
 await expect(page.getByRole('heading',{name:'我的衣橱'})).toBeVisible();
 await page.setViewportSize({width:375,height:800});
 await search(page,'Web 终验');
 await expect(cards(page).first()).toBeVisible();
 const overflow=await page.evaluate(()=>document.documentElement.scrollWidth>window.innerWidth);
 expect(overflow,'375px 不应有页面级横向溢出').toBe(false);
});
