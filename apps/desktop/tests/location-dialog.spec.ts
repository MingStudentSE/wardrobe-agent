import {test,expect} from '@playwright/test';
test('location dialog search, selection and manual coordinates save through core',async({page})=>{
 await page.route('https://geocoding-api.open-meteo.com/**',route=>route.fulfill({json:{results:[{id:1,name:'上海',admin1:'上海市',country:'中国',latitude:31.22,longitude:121.48}]}}));
 await page.goto('/');await page.getByRole('button',{name:'地点',exact:true}).click();const dialog=page.getByRole('dialog',{name:'选择天气地点'});
 await expect(dialog.getByLabel('纬度')).toHaveCount(0);await dialog.getByLabel('城市名称').fill('上海');await dialog.getByRole('button',{name:'搜索',exact:true}).click();await dialog.getByRole('button',{name:/上海市/}).click();await dialog.getByRole('button',{name:'使用此地点'}).click();await expect(dialog).not.toBeVisible();await expect(page.getByRole('region',{name:'地点与天气'})).toContainText('上海');
 await page.getByRole('button',{name:'地点',exact:true}).click();await dialog.getByRole('button',{name:'手填坐标'}).click();await expect(dialog.getByLabel('地点名称')).toHaveValue('上海');await expect(dialog.getByLabel('纬度')).toHaveValue('31.22');
 for(const width of [375,1440]){await page.setViewportSize({width,height:900});await expect.poll(()=>page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);}
 await dialog.getByRole('button',{name:'取消',exact:true}).click();await page.getByRole('button',{name:'地点',exact:true}).click();await page.keyboard.press('Escape');await expect(dialog).not.toBeVisible();
});
