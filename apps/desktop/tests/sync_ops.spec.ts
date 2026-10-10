import {test,expect} from '@playwright/test';
import {fileURLToPath} from 'node:url';
import {mkdtempSync,rmSync,mkdirSync,chmodSync as fsChmodSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';

// #31 同步运维 GUI：状态页（游标/水位/缺图/最近失败）、暂停与恢复、
// 失败批次显式重试、更换目标（复制校验+原子切换）、镜像备份（建立/
// 增量/提升）。后端双设备与真实 WebDAV/S3 路径由 Rust 验收覆盖；
// 此处验证 GUI 入口与真实 Application 状态的一致性。
const openSyncPanel=async(page:import('@playwright/test').Page)=>{
 await page.getByRole('button',{name:'设置',exact:true}).click();
 await page.getByRole('button',{name:/同步.*连接本地同步目标/}).click();
 await page.getByText('跨设备同步',{exact:true}).click();
};

test('同步运维面板：暂停恢复、失败重试、切换目标与镜像备份',async({page,request})=>{
 test.skip(process.env.WARDROBE_SYNC_OS_TEST!=='1','requires actual temporary OS credentials');
 test.setTimeout(180000);
 const remoteA=mkdtempSync(join(tmpdir(),'wardrobe-ops-folder-a-'));
 const remoteB=mkdtempSync(join(tmpdir(),'wardrobe-ops-folder-b-'));
 const remoteM=mkdtempSync(join(tmpdir(),'wardrobe-ops-folder-m-'));
 const callOwner=async(operation:string,input:object={})=>{const response=await request.post('/__wardrobe_dev_api',{data:{operation,input}});expect(response.ok()).toBeTruthy();const body=await response.json();if(body.error)throw new Error(`${body.error.code}: ${body.error.message}`);return body.result;};
 const owner=async(operation:string,input:object={})=>{const result=await callOwner(operation,input);return operation==='sync.run'?(await callOwner('sync.process',{id:result.job_id})).result:result;};
 try{
  await owner('encryption.create',{password:'temporary ops GUI password',device_name:'ops GUI'});
  const member=(await owner('space.default')).member_id;
  await owner('clothing.create',{member_id:member,name:'运维 GUI 衬衫',category:'top'});
  await page.goto('/');
  await openSyncPanel(page);
  const panel=page.locator('#sync-settings');
  await panel.getByLabel('同步文件夹').fill(remoteA);
  await panel.getByRole('button',{name:'保存同步目标',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('同步目标已保存');
  await panel.getByRole('button',{name:'立即同步',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('本次发布',{timeout:60000});
  // 状态页：游标与设备水位对用户可见。
  await expect(panel).toContainText('同步进度：已消费');
  await expect(panel).toContainText('本机水位');
  // 暂停：面板状态与真实 Application 一致（sync.run 显式拒绝）。
  await panel.getByRole('button',{name:'暂停同步',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('同步已暂停：已排队任务保留，恢复后继续');
  const paused=await (await request.post('/__wardrobe_dev_api',{data:{operation:'sync.run',input:{}}})).json();
  expect(paused.error?.code).toBe('sync_paused');
  await panel.getByRole('button',{name:'恢复同步',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('同步已恢复。');
  // 失败批次显式重试：目标不可读时失败可见，恢复后重试收敛。
  fsChmodSync(remoteA,0o000);
  await panel.getByRole('button',{name:'立即同步',exact:true}).click();
  await expect(panel.getByRole('alert').filter({hasText:'上次同步失败'})).toBeVisible({timeout:60000});
  fsChmodSync(remoteA,0o755);
  await panel.getByRole('button',{name:'重试失败的同步',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('本次发布',{timeout:120000});
  await expect(panel.getByRole('alert')).toHaveCount(0);
  // 更换目标：复制校验通过才原子切换；旧目标保留。
  await panel.getByLabel('同步文件夹').fill(remoteB);
  await panel.getByRole('button',{name:'切换到此目标（先复制校验）',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('已通过复制校验并切换同步目标');
  await expect(panel).toContainText(`当前目标：本地文件夹：${remoteB}`);
  const status=await owner('sync.status');
  expect(status.target.directory).toBe(remoteB);
  expect(status.paused).toBe(false);
  // 镜像备份：建立（复制校验）→ 增量 → 提升为主目标。
  await panel.getByText('镜像备份与安全回收',{exact:true}).click();
  await panel.getByLabel('镜像文件夹').fill(remoteM);
  await panel.getByRole('button',{name:'建立镜像并复制校验',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('镜像备份已建立并完成复制校验');
  await expect(panel).toContainText(`镜像备份：本地文件夹：${remoteM}`);
  await owner('clothing.create',{member_id:member,name:'运维 GUI 围巾',category:'top'});
  await panel.getByRole('button',{name:'立即同步',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('本次发布',{timeout:60000});
  await panel.getByRole('button',{name:'同步镜像',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('镜像备份已按主目标增量补齐');
  await panel.getByRole('button',{name:'提升为主目标',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('已校验镜像完整性并提升为主目标');
  await expect(panel).toContainText(`当前目标：本地文件夹：${remoteM}`);
  const promoted=await owner('sync.status');
  expect(promoted.target.directory).toBe(remoteM);
  expect(promoted.mirror.directory).toBe(remoteB);
  await panel.getByRole('button',{name:'立即同步',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('本次发布',{timeout:120000});
  // 键盘可达与窄屏不溢出。
  mkdirSync('../../docs/verification/ticket-31',{recursive:true});
  for(const width of[320,375,414,768,1024,1280,1440]){
   await page.setViewportSize({width,height:1000});
   expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBeTruthy();
   await expect(panel.getByRole('button',{name:'立即同步',exact:true})).toBeVisible();
   if(width===320||width===1280)await panel.screenshot({path:`../../docs/verification/ticket-31/ops-${width}.png`});
  }
 }finally{
  await owner('encryption.unpair');
  fsChmodSync(remoteA,0o755);
  rmSync(remoteA,{recursive:true,force:true});
  rmSync(remoteB,{recursive:true,force:true});
  rmSync(remoteM,{recursive:true,force:true});
 }
});
