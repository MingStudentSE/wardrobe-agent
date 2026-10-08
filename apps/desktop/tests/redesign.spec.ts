import {test,expect} from '@playwright/test';
import {mkdirSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
const shots=fileURLToPath(new URL('../../../design/verification/production/',import.meta.url));mkdirSync(shots,{recursive:true});
test('four workspaces preserve drafts and collection context',async({page})=>{
 await page.goto('/');const nav=page.getByRole('navigation',{name:'主要导航'});await expect(page.locator('.collection .grid article').first()).toBeVisible();
 // 初始只挂载衣橱面板；穿搭库与穿着记录面板按需出现。
 await expect(page.locator('#lookbook')).toHaveCount(0);
 await nav.getByRole('button',{name:'穿搭',exact:true}).click();await expect(page.locator('#lookbook')).toBeVisible();await expect(page.locator('.collection')).toBeHidden();
 await nav.getByRole('button',{name:'衣橱',exact:true}).click();await expect(page.locator('.collection')).toBeVisible();
 // 搭配草稿在切换工作区后保留：在搭配预览填名称，去记录再回来仍在。
 await page.locator('.collection .grid article').first().getByRole('button',{name:'加入搭配'}).click();
 const composerName=page.locator('.outfit-composer input');
 await composerName.fill('尚未保存的组合');
 await nav.getByRole('button',{name:'记录',exact:true}).click();await expect(page.locator('#wears')).toBeVisible();await expect(page.locator('.collection')).toBeHidden();
 // 草稿保留在工作区切换之间：记录→穿搭→设置→衣橱后名称与选中单品仍在。
 await expect(composerName).toHaveValue('尚未保存的组合');
 await nav.getByRole('button',{name:'设置',exact:true}).click();await expect(page.locator('.collection')).toBeHidden();await expect(page.getByRole('region',{name:'设置目录'}).getByRole('button',{name:/备份与恢复/})).toBeVisible();await nav.getByRole('button',{name:'衣橱',exact:true}).click();await expect(page.locator('.collection')).toBeVisible();
 await expect(composerName).toHaveValue('尚未保存的组合');
});
test('create persists through reload; detail opens before edit; discard preserves facts',async({page})=>{
 const name='新版浅蓝衣物 '+Date.now();await page.goto('/');await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();const dialog=page.getByRole('dialog');await page.getByLabel('名称',{exact:true}).fill(name);await page.getByRole('button',{name:'添加到衣橱',exact:true}).click();await expect(dialog).not.toBeVisible();await page.getByLabel('搜索衣物',{exact:true}).fill(name);await page.getByRole('button',{name:'搜索',exact:true}).click();await expect(page.locator('.collection .grid article')).toHaveCount(1);await page.reload();await page.getByLabel('搜索衣物',{exact:true}).fill(name);await page.getByRole('button',{name:'搜索',exact:true}).click();await expect(page.locator('.collection .grid article')).toHaveCount(1);await page.getByRole('button',{name:'查看详情',exact:true}).click();await expect(page.getByRole('region',{name:'单品详情',exact:true})).toBeVisible();await expect(page.getByLabel('名称',{exact:true})).toHaveCount(0);await page.getByRole('button',{name:'编辑衣物',exact:true}).click();await page.getByLabel('名称',{exact:true}).fill('未保存修改');page.once('dialog',d=>d.dismiss());await page.getByRole('button',{name:'关闭单品窗口'}).click();await expect(dialog).toBeVisible();page.once('dialog',d=>d.accept());await page.getByRole('button',{name:'关闭单品窗口'}).click();await expect(dialog).not.toBeVisible();await page.getByRole('button',{name:'查看详情',exact:true}).click();await expect(page.getByRole('region',{name:'单品详情',exact:true})).toContainText(name);await page.getByRole('dialog').getByRole('button',{name:'今天穿了',exact:true}).click();await expect(page.locator('#wears')).toBeVisible();await expect(page.getByRole('dialog',{name:'记录穿着',exact:true})).toBeVisible();await expect(page.getByRole('dialog',{name:'单品详情',exact:true})).toHaveCount(0);
});
test('supported widths show only current workspace without horizontal overflow',async({page})=>{
 for(const width of [320,375,414,768,1024,1280,1440]){await page.setViewportSize({width,height:1000});await page.goto('/');await expect(page.locator('.collection .grid article').first()).toBeVisible();for(const name of ['衣橱','穿搭','记录','设置']){await page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name,exact:true}).click();expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`${name} ${width}`).toBeTruthy();}
 await page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name:'衣橱',exact:true}).click();await page.getByRole('button',{name:'查看详情',exact:true}).first().click();const box=await page.getByRole('dialog').boundingBox();expect(box!.x).toBeGreaterThanOrEqual(0);expect(box!.x+box!.width).toBeLessThanOrEqual(width);await page.keyboard.press('Escape');await expect(page.getByRole('dialog')).not.toBeVisible();await page.screenshot({path:`${shots}wardrobe-${width}.png`,fullPage:true});}
});
test('search and paginated collection remain available after navigating away',async({page})=>{
 await page.goto('/');const seededCount=async(page:any)=>{const r=await(await page.request.post('/__wardrobe_dev_api',{data:{operation:'clothing.list',input:{member_id:'default',status:'active',limit:100}}})).json();return r.result.items.length;};await expect.poll(async()=>await page.locator('.collection .grid article').count(),{timeout:15000}).toBe(Math.min(await seededCount(page),24));await page.getByRole('button',{name:'加载更多',exact:true}).click();await expect.poll(()=>page.locator('.collection .grid article').count()).toBeGreaterThan(Math.min(await seededCount(page),24));const count=await page.locator('.collection .grid article').count();await page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name:'记录',exact:true}).click();await page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name:'衣橱',exact:true}).click();await expect(page.locator('.collection .grid article')).toHaveCount(count);await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();await page.keyboard.press('Escape');await expect(page.getByRole('button',{name:'＋ 新增衣物',exact:true})).toBeFocused();
});
