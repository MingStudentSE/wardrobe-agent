import {test,expect} from '@playwright/test';
import {execFileSync,spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {mkdtempSync,writeFileSync,rmSync,mkdirSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import type {AddressInfo} from 'node:net';

const USER='wardrobe-gui';
const PASS='GUI WebDAV 夹具口令';
// 真实 Apache mod_dav 夹具：Basic 认证 + 条件写入。由本 spec 启停，只停自己
// 启动的进程。需要系统自带 /usr/sbin/httpd（macOS）。
const spawnWebDav=()=>{
 const root=mkdtempSync(join(tmpdir(),'wardrobe-webdav-gui-server-'));
 for(const dir of['data','locks','logs'])mkdirSync(join(root,dir));
 execFileSync('/usr/sbin/htpasswd',['-b','-c','-m',join(root,'htpasswd'),USER,PASS]);
 const free=execFileSync('python3',['-c','import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()']).toString().trim();
 const config=join(root,'httpd.conf');
 const modules='/usr/libexec/apache2';
 writeFileSync(config,`ServerName localhost
Listen 127.0.0.1:${free}
PidFile ${root}/httpd.pid
ErrorLog ${root}/logs/error.log
CustomLog ${root}/logs/access.log "%h %u %>s %r"
LoadModule unixd_module ${modules}/mod_unixd.so
LoadModule mpm_prefork_module ${modules}/mod_mpm_prefork.so
LoadModule authn_core_module ${modules}/mod_authn_core.so
LoadModule authz_core_module ${modules}/mod_authz_core.so
LoadModule auth_basic_module ${modules}/mod_auth_basic.so
LoadModule authn_file_module ${modules}/mod_authn_file.so
LoadModule authz_user_module ${modules}/mod_authz_user.so
LoadModule log_config_module ${modules}/mod_log_config.so
LoadModule dav_module ${modules}/mod_dav.so
LoadModule dav_fs_module ${modules}/mod_dav_fs.so
LoadModule dav_lock_module ${modules}/mod_dav_lock.so
DavLockDB ${root}/locks/DavLock
User ${process.env.USER}
Group staff
DocumentRoot "${root}/data"
<Directory "${root}/data">
  Options Indexes
  AllowOverride None
  Dav On
  AuthType Basic
  AuthName wardrobe-gui
  AuthUserFile ${root}/htpasswd
  Require valid-user
</Directory>
`);
 execFileSync('/usr/sbin/httpd',['-f',config,'-k','start']);
 const deadline=Date.now()+15000;
 for(;;){
  try{execFileSync('curl',['-s','-o','/dev/null','-w','%{http_code}','-u',`${USER}:${PASS}`,'-X','PROPFIND','-H','Depth: 0',`http://127.0.0.1:${free}/`]);break;}catch{}
  if(Date.now()>deadline)throw new Error('GUI WebDAV fixture never became ready');
 }
 return {baseUrl:`http://127.0.0.1:${free}`,root,stop:()=>{
  try{execFileSync('/usr/sbin/httpd',['-f',config,'-k','graceful-stop']);}catch{}
  rmSync(root,{recursive:true,force:true});
 }};
};

test.skip(process.env.WARDROBE_WEBDAV_GUI_TEST!=='1','requires a real local Apache WebDAV server and OS credential store');
test.setTimeout(120000);

test('设置页可配置 WebDAV 同步目标，密码只进本机钥匙串',async({page,request})=>{
 const server=spawnWebDav();
 try{
  // 配对空间：sync.configure 需要已建立的加密身份。
  const callOwner=async(operation:string,input:object={})=>{
   const response=await request.post('/__wardrobe_dev_api',{data:{operation,input}});
   expect(response.ok()).toBeTruthy();
   const body=await response.json();
   if(body.error)throw new Error(body.error.message);
   return body.result;
  };
  await callOwner('encryption.create',{password:'GUI WebDAV 空间口令-唯一短语',device_name:'GUI webdav owner'});
  await page.goto('/');
  await page.getByRole('button',{name:'设置',exact:true}).click();
  await page.getByRole('button',{name:/同步/}).click();
  const panel=page.locator('#sync-settings');
  await panel.getByText('跨设备同步',{exact:true}).click();
  // 等面板初始 reload 完成（尚未配置目标），避免与初始加载竞态。
  await expect(panel.getByText('尚未配置同步目标。')).toBeVisible();
  await panel.getByLabel('目标类型').selectOption('webdav');
  await expect(panel.getByLabel('服务器地址')).toBeVisible();
  await panel.getByLabel('服务器地址').fill(server.baseUrl);
  await panel.getByLabel('根目录').fill('wardrobe-gui-sync');
  await panel.getByLabel('用户名').fill(USER);
  // 错误密码：真实服务器 401 → 明确的认证失败提示。
  await panel.getByLabel('密码').fill('错误的密码');
  await panel.getByRole('button',{name:'保存同步目标',exact:true}).click();
  await expect(panel.getByRole('alert')).toContainText('认证失败');
  // 正确密码：保存成功；状态与输入框都不回显密码。
  await panel.getByLabel('密码').fill(PASS);
  await panel.getByRole('button',{name:'保存同步目标',exact:true}).click();
  await expect(panel.getByRole('status').last()).toContainText('同步目标已保存');
  await expect(panel.getByLabel('密码')).toHaveValue('');
  const status=await callOwner('sync.status');
  expect(status.target.provider_kind).toBe('webdav');
  expect(status.target.url).toBe(server.baseUrl);
  expect(status.target.root).toBe('wardrobe-gui-sync');
  expect(status.target.username).toBe(USER);
  expect(JSON.stringify(status)).not.toContain(PASS);
  await expect(panel.getByRole('button',{name:'立即同步',exact:true})).toBeEnabled();
  // 窄屏不横向溢出（共享视觉语言，兼顾后续 Android）。
  for(const width of[320,375]){await page.setViewportSize({width,height:900});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBeTruthy();await expect(panel.getByLabel('服务器地址')).toBeVisible();}
 }finally{
  try{const response=await request.post('/__wardrobe_dev_api',{data:{operation:'encryption.unpair',input:{}}});await response.text();}catch{}
  server.stop();
 }
});
