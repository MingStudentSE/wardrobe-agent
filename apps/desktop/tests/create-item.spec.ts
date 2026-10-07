import {test,expect} from '@playwright/test';
import {fileURLToPath} from 'node:url';
test('photo first, optional purchase fields, and real asset adoption persist together',async({page})=>{
 await page.goto('/');await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();const dialog=page.getByRole('dialog');
 await expect(dialog.getByLabel('购入价格')).not.toBeVisible();await expect(dialog.getByLabel('材质',{exact:true})).not.toBeVisible();
 await dialog.locator('input[type=file]').setInputFiles(fileURLToPath(new URL('../../../design/studio/assets/jacket.png',import.meta.url)));
 await expect(dialog.getByText('照片已准备好')).toBeVisible();await expect(dialog.getByRole('img',{name:'待添加的衣物照片'})).toBeVisible();
 const name='照片与价格验收 '+Date.now();await dialog.getByLabel('名称',{exact:true}).fill(name);await dialog.getByLabel('类别',{exact:true}).selectOption('outerwear');await dialog.getByLabel('春',{exact:true}).check();
 await dialog.locator('summary').filter({hasText:'购买信息'}).click();await dialog.getByLabel('购入价格').fill('199.90');await dialog.getByLabel('购入日期').fill('2026-10-07');
 await dialog.getByRole('button',{name:'添加到衣橱',exact:true}).click();await expect(dialog).not.toBeVisible();await page.reload();await page.getByLabel('搜索衣物',{exact:true}).fill(name);await page.getByRole('button',{name:'搜索',exact:true}).click();await expect(page.locator('.collection .grid article')).toHaveCount(1);await expect(page.locator('.collection .grid article img')).toBeVisible();await expect(page.locator('.collection .grid article')).toContainText('199.90');await page.getByRole('button',{name:'查看详情',exact:true}).click();await expect(page.getByRole('region',{name:'单品详情',exact:true})).toContainText(name);
});
test('collapsed groups and footer fit compact screens',async({page})=>{
 for(const width of [320,375,1280]){await page.setViewportSize({width,height:720});await page.goto('/');await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();const dialog=page.getByRole('dialog');const box=await dialog.boundingBox();expect(box!.x).toBeGreaterThanOrEqual(0);expect(box!.height).toBeLessThanOrEqual(700);await expect(dialog.getByRole('button',{name:'添加到衣橱',exact:true})).toBeInViewport();await dialog.locator('summary').filter({hasText:'详细属性'}).click();await expect(dialog.getByLabel('材质',{exact:true})).toBeVisible();await expect(dialog.getByRole('button',{name:'取消',exact:true})).toBeInViewport();await dialog.getByRole('button',{name:'取消',exact:true}).click();await expect(dialog).not.toBeVisible();}
});
