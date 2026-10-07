import {test,expect} from '@playwright/test';
test('成员计划创建、重启、暂停与恢复默认本机后台关闭',async({page})=>{
 await page.goto('/');await expect(page.getByRole('combobox',{name:'当前成员',exact:true})).toBeEnabled();
 await page.getByText('定时计划与通知',{exact:true}).click();
 await page.getByLabel('计划名称',{exact:true}).fill('公开GUI护理计划');await page.getByRole('combobox',{name:'计划内容',exact:true}).selectOption('care');await page.getByLabel('本地执行时间',{exact:true}).fill('09:30');await page.getByLabel('计划时区',{exact:true}).fill('Asia/Shanghai');
 await page.getByRole('button',{name:'创建定时计划',exact:true}).click();await expect(page.getByRole('status').filter({hasText:'计划已创建'})).toBeVisible();
 const row=page.getByRole('listitem').filter({hasText:'公开GUI护理计划'});await expect(row).toContainText('启用');await row.getByRole('button',{name:'暂停计划',exact:true}).click();await expect(row).toContainText('暂停');
 await page.reload();await expect(page.getByRole('combobox',{name:'当前成员',exact:true})).toBeEnabled();await page.getByText('定时计划与通知',{exact:true}).click();await expect(page.getByRole('listitem').filter({hasText:'公开GUI护理计划'})).toContainText('暂停');await expect(page.getByLabel('关闭窗口后继续后台运行',{exact:true})).not.toBeChecked();await expect(page.getByLabel('发送系统通知',{exact:true})).not.toBeChecked();await expect(page.getByText('关机或所有进程退出时无法执行',{exact:false})).toBeVisible();
});
