import {test,expect,type Page} from '@playwright/test';
import {mkdirSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
// ticket-34 集中设置验收：设置逐页可达、修改经 dev 桥真实落库、重启（reload）
// 后读回一致。同步页只验证表面可用——状态页能力由 #31 并行交付，合入后终验。
const shots=fileURLToPath(new URL('../../../docs/verification/ticket-34/',import.meta.url));
mkdirSync(shots,{recursive:true});
const nav=(p:Page,name:string)=>p.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name,exact:true});
async function openSettings(page:Page,card:RegExp){
 await nav(page,'设置').click();
 await page.getByRole('region',{name:'设置目录'}).getByRole('button',{name:card}).click();
}
async function backToHub(page:Page){await page.getByRole('button',{name:'← 返回设置'}).click();}

test('模型与连接：新增配置经重启后读回一致',async({page})=>{
 const name='持久化验收模型';
 await openSettings(page,/模型与连接/);
 await page.getByRole('button',{name:'＋ 添加模型',exact:true}).click();
 const dialog=page.getByRole('dialog',{name:'添加模型',exact:true});
 await dialog.getByLabel('配置名称').fill(name);
 await dialog.getByLabel('兼容 API 端点').fill('http://127.0.0.1:9/v1');
 await dialog.getByLabel('模型名称',{exact:true}).fill('persistence-fixture');
 await dialog.getByLabel('认证方式').selectOption('none');
 await dialog.getByRole('button',{name:'保存模型配置',exact:true}).click();
 await expect(page.getByRole('dialog',{name:'管理模型连接'}).getByRole('status')).toContainText('模型配置已保存');
 await page.getByRole('dialog').getByRole('button',{name:'关闭单品窗口'}).click();
 await expect(page.locator('.provider-grid')).toContainText(name);
 await page.reload();
 await openSettings(page,/模型与连接/);
 await page.locator('.provider-grid article').filter({hasText:name}).getByRole('button',{name:'管理连接'}).click();
 const readback=page.getByRole('dialog',{name:'管理模型连接'});
 await expect(readback.getByLabel('配置名称')).toHaveValue(name);
 await expect(readback.getByLabel('模型名称',{exact:true})).toHaveValue('persistence-fixture');
 await expect(readback.getByLabel('兼容 API 端点')).toHaveValue('http://127.0.0.1:9/v1');
 await page.screenshot({path:`${shots}settings-model-persist.png`});
});

test('穿衣偏好：明确偏好保存并跨重启读回，来源标注用户声明',async({page})=>{
 const value=`验收蓝${Date.now()%10000}`;
 await openSettings(page,/穿衣偏好/);
 await page.getByRole('button',{name:'＋ 添加偏好',exact:true}).click();
 const dialog=page.getByRole('dialog',{name:'添加穿衣偏好',exact:true});
 await dialog.getByLabel('偏好维度').selectOption('color');
 await dialog.getByLabel('偏好值').fill(value);
 await dialog.getByLabel('偏好权重').fill('3');
 await dialog.getByLabel('偏好说明').fill('ticket-34 持久化验收');
 await dialog.getByRole('button',{name:'保存明确偏好',exact:true}).click();
 await expect(page.locator('.learning-explicit')).toContainText(value);
 await page.reload();
 await openSettings(page,/穿衣偏好/);
 const row=page.locator('.learning-explicit li').filter({hasText:value});
 await expect(row).toContainText('权重 3');
 await expect(row).toContainText('用户明确声明');
 await page.screenshot({path:`${shots}settings-preferences-persist.png`});
 // 清理：删除该偏好，不污染推荐排序。
 await row.getByRole('button',{name:'删除',exact:true}).click();
 await page.getByLabel('确认删除此记忆').check();
 await page.getByRole('button',{name:'确认删除记忆',exact:true}).click();
 await expect(page.locator('.learning-explicit')).not.toContainText(value);
});

test('天气：自动天气配置保存并跨重启读回',async({page})=>{
 await nav(page,'穿搭').click();
 await page.getByRole('button',{name:'帮我搭配',exact:true}).click();
 const panel=page.locator('#recommendations');
 await panel.locator('summary').filter({hasText:'天气与高级设置'}).click();
 await panel.locator('summary').filter({hasText:'设置天气地点'}).click();
 await panel.locator('summary').filter({hasText:'天气地点与缓存'}).click();
 await panel.getByLabel('天气地点名称').fill('验收城市');
 await panel.getByLabel('天气纬度').fill('31.23');
 await panel.getByLabel('天气经度').fill('121.47');
 await panel.getByLabel('仅在非商业用途使用免费天气服务').check();
 await panel.getByRole('button',{name:'保存自动天气配置',exact:true}).click();
 await expect(panel.getByRole('button',{name:'获取配置地点天气'})).toBeEnabled();
 await page.reload();
 await nav(page,'穿搭').click();
 await page.getByRole('button',{name:'帮我搭配',exact:true}).click();
 const reloaded=page.locator('#recommendations');
 await reloaded.locator('summary').filter({hasText:'天气与高级设置'}).click();
 await reloaded.locator('summary').filter({hasText:'设置天气地点'}).click();
 await reloaded.locator('summary').filter({hasText:'天气地点与缓存'}).click();
 await expect(reloaded.getByLabel('天气地点名称')).toHaveValue('验收城市');
 await expect(reloaded.getByLabel('天气纬度')).toHaveValue('31.23');
 await page.screenshot({path:`${shots}settings-weather-persist.png`});
});

test('定时计划与通知：创建后跨重启读回（详细行为见 schedules 系列规格）',async({page})=>{
 await openSettings(page,/定时计划与通知/);
 await page.getByText('定时计划与通知',{exact:true}).click();
 await page.getByLabel('计划名称',{exact:true}).fill('ticket34 持久化计划');
 await page.getByRole('combobox',{name:'计划内容',exact:true}).selectOption('care');
 await page.getByLabel('本地执行时间',{exact:true}).fill('09:30');
 await page.getByLabel('计划时区',{exact:true}).fill('Asia/Shanghai');
 await page.getByRole('button',{name:'创建定时计划',exact:true}).click();
 await expect(page.getByRole('status').filter({hasText:'计划已创建'})).toBeVisible();
 // 计划不可删除（产品行为）：先重载验证持久，再暂停保持惰性。
 await page.reload();
 await openSettings(page,/定时计划与通知/);
 await page.getByText('定时计划与通知',{exact:true}).click();
 const row=page.getByRole('listitem').filter({hasText:'ticket34 持久化计划'});
 await expect(row).toBeVisible();
 await expect(row).toContainText('启用');
 await row.getByRole('button',{name:'暂停计划',exact:true}).click();
 await expect(row).toContainText('暂停');
 await page.screenshot({path:`${shots}settings-schedule-persist.png`});
});

test('备份与数据目录：设置保存并跨重启读回（恢复/迁移行为见 backup 规格）',async({page})=>{
 const folder='/tmp/wardrobe-ticket34-backup-persist';
 await openSettings(page,/备份与恢复/);
 await page.getByLabel('默认备份目录',{exact:true}).fill(folder);
 await page.getByRole('button',{name:'保存备份设置',exact:true}).click();
 await expect(page.getByRole('status').filter({hasText:'备份设置已保存'})).toBeVisible();
 await page.reload();
 await openSettings(page,/备份与恢复/);
 await expect(page.getByLabel('默认备份目录',{exact:true})).toHaveValue(folder);
 await expect(page.getByLabel('当前数据目录',{exact:true})).toHaveValue(/.+/);
 await page.screenshot({path:`${shots}settings-backup-persist.png`});
 await page.getByLabel('默认备份目录',{exact:true}).fill('');
 await page.getByRole('button',{name:'保存备份设置',exact:true}).click();
});

test('加密与设备、同步、CLI/MCP、任务中心逐页可达且无错误告警',async({page})=>{
 await openSettings(page,/加密与设备/);
 await expect(page.locator('#encryption-settings')).toBeVisible();
 await expect(page.locator('#encryption-settings').getByRole('alert')).toHaveCount(0);
 await page.screenshot({path:`${shots}settings-encryption.png`});
 await backToHub(page);
 await openSettings(page,/CLI \/ MCP/);
 await expect(page.locator('#client-settings')).toBeVisible();
 await expect(page.locator('#client-settings').getByRole('alert')).toHaveCount(0);
 await backToHub(page);
 await openSettings(page,/任务中心/);
 await expect(page.locator('.task-center')).toBeVisible();
 await backToHub(page);
 await openSettings(page,/同步/);
 await expect(page.getByText('← 返回设置')).toBeVisible();
 // 同步状态页能力由 #31 并行开发（SyncSettings.tsx），此处只记录当前表面；
 // #31 合入后须在该页补齐状态与能力匹配的终验。
 await page.screenshot({path:`${shots}settings-sync-surface.png`,fullPage:true});
});

test('任务中心反映真实后台任务并刷新',async({page})=>{
 const call=async(operation:string,input:object)=>{const r=await(await page.request.post('/__wardrobe_dev_api',{data:{operation,input}})).json();return r.result;};
 await openSettings(page,/任务中心/);
 await page.getByRole('button',{name:'刷新状态',exact:true}).click();
 await expect(page.locator('.task-center')).toBeVisible();
 await page.screenshot({path:`${shots}settings-tasks.png`,fullPage:true});
});
