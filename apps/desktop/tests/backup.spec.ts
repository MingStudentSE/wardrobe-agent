import {test,expect} from '@playwright/test';
import {mkdtempSync,rmSync,readFileSync,writeFileSync} from 'node:fs';import {tmpdir} from 'node:os';import {join} from 'node:path';
test('备份范围持久化、实际预览覆盖恢复、保留回退备份与七宽度',async({page,request})=>{
 const folder=mkdtempSync(join(tmpdir(),'wardrobe32-gui-'));
 try{
 await page.goto('/');await page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name:'设置',exact:true}).click();await page.getByRole('region',{name:'设置目录'}).getByRole('button',{name:/备份与恢复/}).click();await page.locator('summary').filter({hasText:'恢复已有备份'}).click();
 await page.getByLabel('默认备份目录',{exact:true}).fill(folder);await page.getByLabel('包含本机原图').check();await page.getByRole('button',{name:'保存备份设置',exact:true}).click();
 await expect(page.getByRole('status').filter({hasText:'备份设置已保存'})).toBeVisible();await page.reload();await page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name:'设置',exact:true}).click();await page.getByRole('region',{name:'设置目录'}).getByRole('button',{name:/备份与恢复/}).click();await page.locator('summary').filter({hasText:'恢复已有备份'}).click();await expect(page.getByLabel('默认备份目录',{exact:true})).toHaveValue(folder);await expect(page.getByLabel('包含本机原图')).toBeChecked();
 await page.getByRole('button',{name:'创建本地备份',exact:true}).click();await expect(page.getByRole('status').filter({hasText:'备份已创建'})).toBeVisible();const packagePath=await page.getByLabel('恢复包目录',{exact:true}).inputValue();expect(packagePath.startsWith(folder)).toBeTruthy();
 const manifestPath=join(packagePath,'manifest.json');const originalManifest=readFileSync(manifestPath,'utf8');const invalidManifest=JSON.parse(originalManifest);invalidManifest.version=999999;writeFileSync(manifestPath,JSON.stringify(invalidManifest));await page.getByRole('button',{name:'预览恢复范围',exact:true}).click();await expect(page.locator('#backup-settings [role="alert"]')).toBeVisible();await expect(page.getByRole('button',{name:'覆盖恢复当前衣橱',exact:true})).toHaveCount(0);writeFileSync(manifestPath,originalManifest);
 const after=await request.post('/__wardrobe_dev_api',{data:{operation:'clothing.create',input:{member_id:'default',name:'备份后新增，不应留在覆盖库',category:'top'}}});const removed=(await after.json()).result;
 await page.getByRole('button',{name:'预览恢复范围',exact:true}).click();await expect(page.getByText('恢复前先看清范围',{exact:true})).toBeVisible();await expect(page.getByRole('button',{name:'覆盖恢复当前衣橱',exact:true})).toBeDisabled();await page.getByLabel('确认覆盖当前事实并保留预备备份').check();await page.getByRole('button',{name:'覆盖恢复当前衣橱',exact:true}).click();await expect(page.getByRole('status').filter({hasText:'恢复完成'})).toBeVisible();await expect(page.getByLabel('回退备份目录',{exact:true})).not.toHaveValue('');
 const read=await request.post('/__wardrobe_dev_api',{data:{operation:'clothing.get',input:{member_id:'default',id:removed.id}}});expect((await read.json()).error.code).toBe('not_found');
 for(const width of [320,375,414,768,1024,1280,1440]){await page.setViewportSize({width,height:900});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBeTruthy();if(width===320||width===1280)await page.locator('#backup-settings').screenshot({path:`../../docs/verification/ticket-32/backup-${width}.png`});}
 }finally{rmSync(folder,{recursive:true,force:true});}
});

test('独立安装实际移动目录后 GUI 与旧路径 CLI 读取同一数据',async({page})=>{
 const {spawn,execFileSync}=await import('node:child_process');const {createServer}=await import('node:net');const {fileURLToPath}=await import('node:url');
 const folder=mkdtempSync(join(tmpdir(),'wardrobe32-directory-gui-'));const source=join(folder,'source');const destination=join(folder,'moved');const db=join(source,'wardrobe.sqlite');
 const cli=fileURLToPath(new URL('../../../target/debug/wardrobe-cli',import.meta.url));
 const call=(name:string,input:object)=>JSON.parse(execFileSync(cli,['--db',db,'--trusted-local',name,JSON.stringify(input)],{encoding:'utf8'})).result;
 call('space.default',{});const garment=call('clothing.create',{member_id:'default',name:'目录迁移的实际衣物',category:'top'});
 const listener=createServer();await new Promise<void>(resolve=>listener.listen(0,'127.0.0.1',resolve));const port=(listener.address() as {port:number}).port;await new Promise<void>(resolve=>listener.close(()=>resolve()));
 const server=spawn(process.execPath,[fileURLToPath(new URL('../node_modules/vite/bin/vite.js',import.meta.url)),'--host','127.0.0.1','--port',String(port),'--strictPort'],{cwd:fileURLToPath(new URL('..',import.meta.url)),env:{...process.env,WARDROBE_DATA_DIR:source},stdio:'ignore'});
 try{
  const url=`http://127.0.0.1:${port}`;await expect.poll(async()=>{try{return (await fetch(url)).status;}catch{return 0;}},{timeout:15000}).toBe(200);
  await page.goto(url);await page.getByRole('navigation',{name:'主要导航'}).getByRole('button',{name:'设置',exact:true}).click();await page.getByRole('region',{name:'设置目录'}).getByRole('button',{name:/备份与恢复/}).click();await page.locator('summary').filter({hasText:'移动本机数据目录'}).click();await page.getByLabel('新数据目录',{exact:true}).fill(destination);await expect(page.getByRole('button',{name:'复制验证并切换目录',exact:true})).toBeDisabled();await page.getByLabel('确认复制验证后切换并保留旧目录').check();await page.getByRole('button',{name:'复制验证并切换目录',exact:true}).click();await expect(page.getByRole('status').filter({hasText:'数据目录已切换'})).toBeVisible();await expect(page.getByLabel('当前数据目录',{exact:true})).toHaveValue(/moved$/);
  expect(call('clothing.get',{member_id:'default',id:garment.id})).toEqual(garment);
  const created=call('clothing.create',{member_id:'default',name:'旧路径工具迁移后新增',category:'shoes'});await page.reload();await expect(page.getByRole('heading',{name:created.name,exact:true})).toBeVisible();
 }finally{server.kill('SIGTERM');await new Promise<void>(resolve=>{if(server.exitCode!==null)resolve();else server.once('exit',()=>resolve());});rmSync(folder,{recursive:true,force:true});}
});
