import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {test,expect} from '@playwright/test';
import {mkdtempSync,readdirSync,readFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
test('真实系统密钥导出独立密文包，窄屏可操作并验证读取',async({page,request})=>{
 test.skip(process.env.WARDROBE_ENCRYPTION_OS_TEST!=='1','requires actual temporary OS credential');
 const folder=mkdtempSync(join(tmpdir(),'wardrobe-gui-export-'));
 try {
 const created=await request.post('/__wardrobe_dev_api',{data:{operation:'encryption.create',input:{password:'temporary GUI export password',device_name:'GUI export'}}});expect(created.ok()).toBeTruthy();
 await page.goto('/');await page.getByText('加密空间与设备',{exact:true}).click();
 await page.getByText('独立密文文件包',{exact:true}).click();
 await page.getByLabel('导出目标文件夹').fill(folder);
 await page.getByRole('button',{name:'导出密文文件包',exact:true}).click();
 await expect(page.getByRole('status').filter({hasText:'密文包已导出'})).toBeVisible();
 const packagePath=join(folder,readdirSync(folder)[0]);
 expect(readdirSync(packagePath).length).toBeGreaterThanOrEqual(2);
 for(const name of readdirSync(packagePath))expect(readFileSync(join(packagePath,name)).includes(Buffer.from('分页单品'))).toBeFalsy();
 await page.getByLabel('密文包文件夹').fill(packagePath);
 await page.getByRole('button',{name:'验证读取密文包',exact:true}).click();
 await expect(page.getByRole('status').filter({hasText:'密文包已验证'})).toBeVisible();
 for(const width of [320,375,414,768,1024,1280,1440]){await page.setViewportSize({width,height:900});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBeTruthy();await expect(page.getByRole('button',{name:'导出密文文件包',exact:true})).toBeVisible();if(width===320||width===1280)await page.locator('#encryption-settings').screenshot({path:`../../docs/verification/ticket-26-export/package-${width}.png`});}
 }finally{execFileSync(fileURLToPath(new URL('../../../target/debug/wardrobe-cli',import.meta.url)),['--trusted-local','encryption.unpair','--input-stdin'],{input:'{}',env:{...process.env,WARDROBE_DATA_DIR:process.env.WARDROBE_TEST_DATA_DIR}});rmSync(folder,{recursive:true,force:true});}
});
