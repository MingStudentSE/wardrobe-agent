#!/usr/bin/env node
// 部署演练冒烟（ADR0029 D8）：对「已部署」的 wardrobe-web 实例用真实 Chrome 跑核心流程。
// 用法：
//   node deploy-smoke.mjs bootstrap --url http://127.0.0.1:PORT --data-dir <数据目录> --photo <图片> --name <单品名>
//   node deploy-smoke.mjs resume    --url http://127.0.0.1:PORT --data-dir <数据目录> --name <单品名>
// bootstrap：读一次性引导令牌 → 网页首启 → 保存连接令牌 → 界面录入真实照片单品 → 重载断言。
// resume：服务重启后（会话不持久化）用连接令牌重新登录 → 断言数据完好。
import {chromium} from 'playwright';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {dirname,join} from 'node:path';

function arg(name,fallback){
 const i=process.argv.indexOf(`--${name}`);
 return i>=0?process.argv[i+1]:fallback;
}
const stage=process.argv[2];
const url=arg('url','http://127.0.0.1:17838');
const dataDir=arg('data-dir');
const photo=arg('photo');
const name=arg('name','部署演练夹克');
if(!dataDir||!stage||(stage==='bootstrap'&&!photo)){
 console.error('用法见文件头注释：需要 --data-dir；bootstrap 还需要 --photo');
 process.exit(2);
}
mkdirSync(dataDir,{recursive:true});
const tokenFile=join(dataDir,'deploy-connection-token.txt');
const stateFile=join(dataDir,'deploy-state.json');

const browser=await chromium.launch({channel:'chrome',headless:true});
try{
 if(stage==='bootstrap'){
  const context=await browser.newContext({locale:'zh-CN'});
  const page=await context.newPage();
  await page.goto(url);
  await expect2(page.getByRole('heading',{name:'首次使用：登记本机浏览器'}));
  const token=readFileSync(join(dataDir,'web-bootstrap-token'),'utf8').trim();
  await page.getByLabel('引导令牌').fill(token);
  await page.getByRole('button',{name:'登记本机浏览器'}).click();
  const box=page.getByLabel('本浏览器的连接令牌（只显示这一次）');
  await expect2(box);
  const connection=await box.inputValue();
  if(connection.length!==64)throw new Error('连接令牌长度异常');
  writeFileSync(tokenFile,connection);
  await page.getByRole('button',{name:'我已保存，进入衣橱'}).click();
  await expect2(page.getByRole('heading',{name:'我的衣橱'}));
  // 真实照片录入 → 重载恢复
  await page.setViewportSize({width:1280,height:1600});
  await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();
  const dialog=page.getByRole('dialog');
  await dialog.getByRole('region',{name:'录入单品'}).locator('input[type=file]').setInputFiles(photo);
  await expect2(dialog.getByText('照片已准备好'));
  await dialog.getByLabel('名称',{exact:true}).fill(name);
  await dialog.getByRole('button',{name:'添加到衣橱',exact:true}).click();
  await page.getByRole('dialog').waitFor({state:'hidden',timeout:30000});
  await page.getByLabel('搜索衣物',{exact:true}).first().fill(name);
  await page.getByRole('button',{name:'搜索',exact:true}).click();
  await expect2(page.locator('.collection .grid article').first());
  await page.reload();
  await page.getByLabel('搜索衣物',{exact:true}).first().fill(name);
  await page.getByRole('button',{name:'搜索',exact:true}).click();
  await expect2(page.locator('.collection .grid article').first());
  await context.storageState({path:stateFile});
  writeFileSync(join(dataDir,'deploy-item.txt'),name);
  console.log(`bootstrap 阶段通过：已登记浏览器、录入「${name}」并重载恢复`);
 }else if(stage==='resume'){
  const connection=readFileSync(tokenFile,'utf8').trim();
  const context=await browser.newContext({locale:'zh-CN',storageState:{cookies:[],origins:[]}});
  const page=await context.newPage();
  await page.goto(url);
  // 服务重启后内存会话清空：必须回到登录门（如实标注的行为）
  await expect2(page.getByRole('heading',{name:'登录本机衣橱'}));
  await page.getByLabel('连接令牌').fill(connection);
  await page.getByRole('button',{name:'登录'}).click();
  await expect2(page.getByRole('heading',{name:'我的衣橱'}));
  const name2=readFileSync(join(dataDir,'deploy-item.txt'),'utf8').trim()||name;
  await page.getByLabel('搜索衣物',{exact:true}).first().fill(name2);
  await page.getByRole('button',{name:'搜索',exact:true}).click();
  await expect2(page.locator('.collection .grid article').first());
  await expect2(page.locator('.collection .grid article').first().locator('img'));
  await context.storageState({path:stateFile});
  console.log(`resume 阶段通过：连接令牌重登录成功，升级前录入的「${name2}」与照片完好`);
 }else{
  throw new Error(`未知阶段：${stage}`);
 }
}finally{
 await browser.close();
}

async function expect2(locator,timeout=30000){
 await locator.waitFor({state:'visible',timeout});
}
