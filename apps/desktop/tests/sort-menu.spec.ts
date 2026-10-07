import {test,expect} from '@playwright/test';
test('sort menu opens below trigger and supports selection and keyboard dismissal',async({page})=>{
 await page.goto('/');const trigger=page.getByRole('button',{name:'排序：录入顺序'});await trigger.click();const menu=page.getByRole('menu',{name:'排序方式'});await expect(menu).toBeVisible();const t=await trigger.boundingBox(),m=await menu.boundingBox();expect(m!.y).toBeGreaterThanOrEqual(t!.y+t!.height+7);
 await page.getByRole('menuitemradio',{name:'名称',exact:true}).click();await expect(page.getByRole('button',{name:'排序：名称'})).toBeVisible();await expect(menu).not.toBeVisible();await page.getByRole('button',{name:'排序：名称'}).click();await page.keyboard.press('Escape');await expect(menu).not.toBeVisible();await expect(page.getByRole('button',{name:'排序：名称'})).toBeFocused();
 await page.getByRole('button',{name:'排序：名称'}).click();await page.getByRole('heading',{name:'我的衣橱'}).click();await expect(menu).not.toBeVisible();
 await page.setViewportSize({width:375,height:900});await page.getByRole('button',{name:'排序：名称'}).click();expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
});
