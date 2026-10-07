import {test,expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {mkdtempSync,writeFileSync,readdirSync,renameSync,rmSync,mkdirSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
const openSyncPanel=async(page:import('@playwright/test').Page)=>{
 await page.getByRole('button',{name:'设置',exact:true}).click();
 await page.getByRole('button',{name:/同步.*连接本地同步目标/}).click();
 await page.getByText('跨设备同步',{exact:true}).click();
};
test('同步目标无配置时真实Application状态与窄屏手工配置入口',async({page})=>{
 await page.goto('/');
 await openSyncPanel(page);
 const panel=page.locator('#sync-settings');
 await expect(panel.getByLabel('同步文件夹')).toBeVisible();
 await expect(panel.getByRole('button',{name:'立即同步',exact:true})).toBeDisabled();
 await panel.getByLabel('同步文件夹').fill('/this-directory-does-not-exist-wardrobe-sync');
 await panel.getByRole('button',{name:'保存同步目标',exact:true}).click();
 await expect(panel.getByRole('alert')).toBeVisible();
 for(const width of[320,375,414,768,1024,1280,1440]){await page.setViewportSize({width,height:900});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBeTruthy();await expect(panel.getByRole('button',{name:'保存同步目标',exact:true})).toBeVisible();}
});

test('实际双安装GUI配置、断网名称冲突人工选择与缺图重试',async({page,request})=>{
 test.skip(process.env.WARDROBE_SYNC_OS_TEST!=='1','requires actual temporary OS credentials');test.setTimeout(180000);
 const peerDir=mkdtempSync(join(tmpdir(),'wardrobe-sync-gui-peer-')),remote=mkdtempSync(join(tmpdir(),'wardrobe-sync-gui-folder-'));
 const cli=fileURLToPath(new URL('../../../target/debug/wardrobe-cli',import.meta.url));
 const callPeer=(operation:string,input:object={})=>JSON.parse(execFileSync(cli,['--trusted-local',operation,'--input-stdin'],{input:JSON.stringify(input),env:{...process.env,WARDROBE_DATA_DIR:peerDir}}).toString()).result;
 const peer=(operation:string,input:object={})=>{const result=callPeer(operation,input);return operation==='sync.run'?callPeer('sync.process',{id:result.job_id}).result:result;};
 const callOwner=async(operation:string,input:object={})=>{const response=await request.post('/__wardrobe_dev_api',{data:{operation,input}});expect(response.ok()).toBeTruthy();const body=await response.json();if(body.error)throw new Error(body.error.message);return body.result;};
 const owner=async(operation:string,input:object={})=>{const result=await callOwner(operation,input);return operation==='sync.run'?(await callOwner('sync.process',{id:result.job_id})).result:result;};
 try {
 const member=(await owner('space.default')).member_id;
 await owner('encryption.create',{password:'temporary GUI sync password',device_name:'GUI sync owner'});
 const offer=await owner('encryption.pair.offer',{device_name:'GUI peer'});
 const receipt=peer('encryption.pair.accept',{invitation:offer.invitation,pairing_secret:offer.pairing_secret,password:'temporary GUI peer password'});
 const confirmation=await owner('encryption.pair.confirm',{receipt:receipt.receipt});peer('encryption.device.apply',{manifest:confirmation.manifest});
 const garment=await owner('clothing.create',{member_id:member,name:'GUI owner original',category:'top'});
 for(let i=0;i<260;i++)await owner('clothing.create',{member_id:member,name:`GUI bounded batch ${i}`,category:'top'});
 peer('sync.configure',{provider_kind:'local_folder',directory:remote});
 await page.goto('/');await openSyncPanel(page);const panel=page.locator('#sync-settings');
 await panel.getByLabel('同步文件夹').fill(remote);await panel.getByRole('button',{name:'保存同步目标',exact:true}).click();await expect(panel.getByRole('status')).toContainText('同步目标已保存');
 let release!:()=>void,markReady!:()=>void;const held=new Promise<void>(r=>{release=r;}),ready=new Promise<void>(r=>{markReady=r;});let holdOnce=true;
 await page.route('**/__wardrobe_dev_api',async route=>{if(holdOnce&&route.request().postDataJSON()?.operation==='sync.run'){holdOnce=false;const response=await route.fetch();markReady();await held;await route.fulfill({response});}else await route.continue();});
 await panel.getByRole('button',{name:'立即同步',exact:true}).click();await ready;await panel.getByRole('button',{name:'停止后续批次',exact:true}).click();release();await expect(panel.getByRole('status')).toContainText('同步已停止');await page.unroute('**/__wardrobe_dev_api');
 const cancelled=await callOwner('jobs.list',{member_id:member});expect(cancelled.items.some((job:{kind:string;state:string})=>job.kind==='sync'&&job.state==='cancelled')).toBeTruthy();
 await panel.getByRole('button',{name:'立即同步',exact:true}).click();await expect(panel.getByRole('status')).toContainText('本次发布',{timeout:60000});await expect(panel.getByRole('button',{name:'立即同步',exact:true})).toBeEnabled({timeout:60000});expect((await owner('sync.run')).sent).toBe(0);while(peer('sync.run').has_more){};
 await owner('clothing.update',{member_id:member,id:garment.id,name:'GUI owner candidate'});peer('clothing.update',{member_id:member,id:garment.id,name:'GUI peer candidate'});
 await owner('sync.run');peer('sync.run');await owner('sync.run');
 await page.reload();await openSyncPanel(page);
 const conflict=panel.locator('.sync-conflicts>li').filter({hasText:'单品 · 名称'});await expect(conflict).toContainText('GUI owner candidate');await expect(conflict).toContainText('GUI peer candidate');
 mkdirSync('../../docs/verification/ticket-27',{recursive:true});
 for(const width of[320,375,414,768,1024,1280,1440]){await page.setViewportSize({width,height:1000});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBeTruthy();if(width===320||width===1280)await panel.screenshot({path:`../../docs/verification/ticket-27/conflicts-${width}.png`});}
 await conflict.getByRole('button',{name:'选择候选 1',exact:true}).click();await panel.getByRole('button',{name:'确认采用选择',exact:true}).click();await expect(panel.getByRole('status')).toContainText('选择已保存');
 await panel.getByRole('button',{name:'立即同步',exact:true}).click();await expect(panel.getByRole('status')).toContainText('本次发布');peer('sync.run');
 expect(peer('clothing.get',{member_id:member,id:garment.id}).name).toBe((await owner('clothing.get',{member_id:member,id:garment.id})).name);
 const source=join(peerDir,'private-peer-original.png');const png=await page.evaluate(()=>{const c=document.createElement('canvas');c.width=64;c.height=64;const ctx=c.getContext('2d')!;ctx.fillStyle='#21445c';ctx.fillRect(0,0,64,64);return c.toDataURL('image/png').split(',')[1];});writeFileSync(source,Buffer.from(png,'base64'));
 const image=peer('assets.import',{member_id:member,path:source});peer('assets.adopt',{member_id:member,clothing_id:garment.id,asset_id:image.id});peer('sync.run');
 function findChunk(folder:string):string|undefined{for(const name of readdirSync(folder,{withFileTypes:true})){const p=join(folder,name.name);if(name.isDirectory()){const found=findChunk(p);if(found)return found;}else if(folder.endsWith('/blobs')&&name.name.endsWith('.object'))return p;}return undefined;}
 const chunk=findChunk(remote)!;expect(chunk).toBeTruthy();const backup=join(peerDir,'temporarily-offline-chunk');renameSync(chunk,backup);
 await panel.getByRole('button',{name:'立即同步',exact:true}).click();await expect(panel).toContainText('还有 1 张展示图等待重试');await expect(panel.getByRole('button',{name:'重试缺失展示图',exact:true})).toBeEnabled();
 renameSync(backup,chunk);await panel.getByRole('button',{name:'重试缺失展示图',exact:true}).click();await expect(panel.getByRole('button',{name:'立即同步',exact:true})).toBeEnabled({timeout:60000});await expect(panel.getByRole('button',{name:'重试缺失展示图',exact:true})).toBeDisabled();
 const adopted=await owner('assets.get',{member_id:member,id:image.id});expect(adopted.original).toBeNull();expect((await owner('assets.preview',{member_id:member,id:image.id,variant:'thumbnail'})).data_url.startsWith('data:image/webp;base64,')).toBeTruthy();
 }finally{await owner('encryption.unpair');peer('encryption.unpair');rmSync(peerDir,{recursive:true,force:true});rmSync(remote,{recursive:true,force:true});}
});
