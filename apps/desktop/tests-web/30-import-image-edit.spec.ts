import {test,expect} from '@playwright/test';
import {fileURLToPath} from 'node:url';

// 多图录入与图片编辑（Web 终验）：照片管理多图导入→逐张采用、主图切换、排序、旋转预览放弃，
// 以及 AI 识别在无视觉模型时的能力如实声明（不算失败，断言文案出现）。
// 夹具图片不入 git（用户私人资产），由执行环境从主工作区拷贝。
const jacket=fileURLToPath(new URL('../../../design/studio/assets/jacket.png',import.meta.url));
const tee=fileURLToPath(new URL('../../../design/studio/assets/tee.png',import.meta.url));
const shoes=fileURLToPath(new URL('../../../design/studio/assets/shoes.png',import.meta.url));

test.setTimeout(240_000);

const cards=page=>page.locator('.collection .grid article');

async function openEditor(page:import('@playwright/test').Page){
 await page.getByLabel('搜索衣物',{exact:true}).first().fill('Web 终验多图开衫');
 await page.getByRole('button',{name:'搜索',exact:true}).click();
 await cards(page).first().getByRole('button',{name:'查看详情'}).click();
 await page.getByRole('dialog',{name:'单品详情'}).getByRole('button',{name:'编辑衣物'}).click();
 const editor=page.getByRole('dialog',{name:'编辑衣物'});
 await expect(editor).toBeVisible();
 return editor;
}
async function openPhotosTab(page:import('@playwright/test').Page){
 const editor=await openEditor(page);
 await editor.getByRole('button',{name:'照片管理'}).click();
 return editor;
}

test('multi-photo import, main image switch, reorder and rotate preview on the web',async({page})=>{
 await page.goto('/');
 await expect(page.getByRole('heading',{name:'我的衣橱'})).toBeVisible();

 // 建一件无图单品，再到照片管理批量导入两张真实照片
 await page.getByRole('button',{name:'＋ 新增衣物',exact:true}).click();
 let dialog=page.getByRole('dialog');
 await dialog.getByRole('region',{name:'录入单品'}).getByLabel('名称',{exact:true}).fill('Web 终验多图开衫');
 await dialog.getByRole('button',{name:'添加到衣橱',exact:true}).click();
 await expect(dialog).not.toBeVisible();

 const editor=await openPhotosTab(page);
 // 浏览器文件选择（动态 input）→ /api/upload → assets.import → 逐张采用
 const chooserPromise=page.waitForEvent('filechooser');
 await editor.getByRole('button',{name:'＋ 添加照片'}).click();
 const chooser=await chooserPromise;
 await chooser.setFiles([tee,shoes]);
 const adoptButtons=editor.getByRole('button',{name:'采用为主图'});
 await expect(adoptButtons).toHaveCount(2,{timeout:60000});
 for(let index=0;index<2;index++){
  await adoptButtons.nth(index).click();
  await expect(editor.getByText('展示图已采用，原图保留在本设备。').first()).toBeVisible({timeout:30000});
 }
 const gallery=editor.getByRole('region',{name:'已关联图片'});
 await expect(gallery.locator('.photo-asset-card')).toHaveCount(2);
 // 每次采用都会把该图设为主图：后采用的第二张应为当前主图
 await expect(gallery.locator('.photo-asset-card').nth(1)).toContainText('当前主图',{timeout:30000});

 // 主图切换：把第一张重新设为主图
 await gallery.locator('.photo-asset-card').nth(0).getByRole('button',{name:'设为主图'}).click();
 await expect(gallery.locator('.photo-asset-card').nth(0)).toContainText('当前主图',{timeout:30000});

 // 排序：第一张后移，主图标记跟随图片本身（不随位置变化）
 await gallery.locator('.photo-asset-card').nth(0).getByRole('button',{name:'向后移动'}).click();
 await expect(gallery.locator('.photo-asset-card').nth(1)).toContainText('当前主图',{timeout:30000});
 await expect(gallery.locator('.photo-asset-card').nth(0)).toContainText('图片 1',{timeout:30000});

 // 图片编辑：旋转生成预览，检查后放弃（原图与关联保持）。此时第一张是非主图（被后移）。
 const firstCard=gallery.locator('.photo-asset-card').nth(0);
 await firstCard.getByText('编辑此图片',{exact:true}).click();
 await firstCard.getByRole('button',{name:'顺时针旋转'}).click();
 const preview=firstCard.getByRole('region',{name:'图片编辑预览'});
 await expect(preview).toBeVisible({timeout:30000});
 await preview.getByRole('button',{name:'放弃预览'}).click();
 await expect(firstCard).toContainText('未采用预览，原关联保持。');

 // 重载持久：两图关联与主图仍在（主图是后移到第二位的那张）
 await page.getByRole('dialog',{name:'编辑衣物'}).getByRole('button',{name:'关闭单品窗口'}).click();
 await page.reload();
 await openPhotosTab(page);
 await expect(page.getByRole('dialog',{name:'编辑衣物'}).locator('.photo-asset-card')).toHaveCount(2);
 await expect(page.getByRole('dialog',{name:'编辑衣物'}).locator('.photo-asset-card').nth(1)).toContainText('当前主图');
 await page.getByRole('dialog',{name:'编辑衣物'}).getByRole('button',{name:'关闭单品窗口'}).click();
 await expect(page.getByRole('dialog')).not.toBeVisible();
});

test('AI recognition entry states honestly that no vision model is configured',async({page})=>{
 test.info().annotations.push({type:'capability',description:'模型依赖功能在 Web 的入口做能力如实标注验证：无凭据时如实反馈，不算失败'});
 await page.goto('/');
 await openPhotosTab(page);
 const editor=page.getByRole('dialog',{name:'编辑衣物'});
 // 外层 details 与内层 Recognition details 各有自己的 summary，需逐层展开
 await editor.locator('details.photo-recognition > summary').click();
 await editor.locator('details.recognition > summary').click();
 await expect(editor.getByText('请在模型设置添加视觉模型。')).toBeVisible();
 await expect(editor.getByRole('combobox',{name:'识别模型'})).toHaveValue('');
 // 两张已关联图片在识别入口可见、默认未选中
 await expect(editor.getByRole('checkbox',{name:/发送此图片/})).toHaveCount(2);
 await editor.getByRole('button',{name:'关闭单品窗口'}).click();
 await expect(page.getByRole('dialog')).not.toBeVisible();
});

test('photo import rejects bad types in the browser and keeps association intact',async({page})=>{
 // 负向：文本文件伪装图片，服务端类型白名单拒绝，界面给出错误反馈（不落残留）
 await page.goto('/');
 const editor=await openPhotosTab(page);
 const chooserPromise=page.waitForEvent('filechooser');
 await editor.getByRole('button',{name:'＋ 添加照片'}).click();
 const chooser=await chooserPromise;
 await chooser.setFiles({name:'not-an-image.txt',mimeType:'text/plain',buffer:Buffer.from('这不是图片')});
 await expect(editor.locator('.import-candidates li').getByRole('alert')).toContainText('请选择 JPG、PNG、WebP 或 HEIC 图片',{timeout:30000});
 await editor.getByRole('button',{name:'关闭单品窗口'}).click();
 await page.reload();
 await openPhotosTab(page);
 await expect(page.getByRole('dialog',{name:'编辑衣物'}).locator('.photo-asset-card')).toHaveCount(2);
 await page.getByRole('dialog',{name:'编辑衣物'}).getByRole('button',{name:'关闭单品窗口'}).click();
});
