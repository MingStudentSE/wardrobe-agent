import {test,expect,type Page} from '@playwright/test';
import {fileURLToPath} from 'node:url';
// 单品图片工作区验收（新 UI）：详情 → 编辑衣物 → 照片管理；浏览器通过
// file chooser 提供文件。行为断言（预览、采用、排序、移除、重启保留、
// 失败隔离与重试、上传中成员锁）保持不变。
const paths=[fileURLToPath(new URL('../../../crates/wardrobe-core/tests/fixtures/generated-colors.png',import.meta.url)),fileURLToPath(new URL('../../../crates/wardrobe-core/tests/fixtures/generated-colors.heic',import.meta.url))];
async function openPhotoTab(page:Page){
 await page.getByRole('button',{name:'编辑衣物',exact:true}).click();
 await page.getByRole('dialog').getByRole('button',{name:'照片管理',exact:true}).click();
 const workspace=page.getByRole('region',{name:'单品图片',exact:true});
 await expect(workspace).toBeVisible();
 return workspace;
}
async function addPhotos(page:Page,files:string[]){
 const chooserPromise=page.waitForEvent('filechooser');
 await page.getByRole('button',{name:'＋ 添加照片',exact:true}).click();
 const chooser=await chooserPromise;
 await chooser.setFiles(files);
}
test('real images preview adopt reorder detach and retain references after editing',async({page})=>{
 const second=fileURLToPath(new URL('../../../design/studio/assets/jacket.png',import.meta.url));
 await page.goto('/');await page.getByRole('article').first().getByRole('button',{name:'查看详情'}).click();
 await openPhotoTab(page);
 await addPhotos(page,[paths[0],second]);
 await expect(page.getByRole('img',{name:'待采用的衣物展示图'})).toHaveCount(2);
 await page.getByRole('button',{name:'采用为主图'}).first().click();await expect(page.getByRole('status').filter({hasText:'展示图已采用'})).toContainText('展示图已采用');await page.getByRole('button',{name:'采用为主图'}).last().click();await expect(page.getByRole('status').filter({hasText:'展示图已采用'})).toContainText('展示图已采用');
 const gallery=page.getByRole('region',{name:'已关联图片'});await expect(gallery.getByRole('img')).toHaveCount(2);await gallery.getByRole('button',{name:'向前移动'}).last().click();await expect(gallery.getByRole('status').filter({hasText:'排序已保存'})).toContainText('排序已保存');
 // 名称在基本资料分区：分区切换后保存，再回照片管理做移除。
 await page.getByRole('button',{name:'基本资料',exact:true}).click();
 await page.getByLabel('名称',{exact:true}).fill('图片引用验收');await page.getByRole('button',{name:'保存修改'}).click();await expect(page.getByRole('status').filter({hasText:'已更新'})).toBeVisible();
 await page.getByRole('button',{name:'照片管理',exact:true}).click();
 await expect(gallery.getByRole('img')).toHaveCount(2);await gallery.getByRole('button',{name:'移除关联'}).first().click();await page.getByRole('button',{name:'确认移除关联'}).click();await expect(gallery.getByRole('img')).toHaveCount(1);
 await page.reload();await page.getByRole('article').filter({hasText:'图片引用验收'}).getByRole('button',{name:'查看详情'}).click();await openPhotoTab(page);await expect(page.getByRole('region',{name:'已关联图片'}).getByRole('img')).toHaveCount(1);
});

test('image controls remain accessible at all specified widths',async({page})=>{
 const second=fileURLToPath(new URL('../../../design/studio/assets/jacket.png',import.meta.url));
 await page.goto('/');await page.getByRole('article').first().getByRole('button',{name:'查看详情'}).click();
 await openPhotoTab(page);
 await addPhotos(page,[paths[0],second]);
 await expect(page.getByRole('img',{name:'待采用的衣物展示图'})).toHaveCount(2);await page.getByRole('button',{name:'采用为主图'}).first().click();await expect(page.getByRole('region',{name:'已关联图片'}).getByRole('img').first()).toBeVisible();
 for(const width of [320,375,414,768,1024,1280,1440]) {await page.setViewportSize({width,height:900});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBeTruthy();await expect(page.getByRole('button',{name:'＋ 添加照片'})).toBeVisible();await page.screenshot({path:`/tmp/wardrobe-media-ui-${width}.png`,fullPage:true});await page.getByRole('region',{name:'单品图片',exact:true}).screenshot({path:`/tmp/wardrobe-media-detail-${width}.png`});}
});
test('pending native picker blocks member switching and simultaneous save',async({page})=>{
 // 桥接上传延迟 1.5s：上传进行中（照片工作区 busy）必须锁住成员切换与同弹窗保存。
 await page.route(/__wardrobe_dev_upload/,async route=>{await new Promise(resolve=>setTimeout(resolve,3000));await route.continue();});
 await page.goto('/');await page.getByText('当前衣橱',{exact:true}).click();await page.getByRole('article').first().getByRole('button',{name:'查看详情'}).click();
 await openPhotoTab(page);
 await addPhotos(page,[paths[0]]);
 // 上传进行中：成员切换与同弹窗保存都被锁定，完成后恢复。
 await expect(page.getByRole('combobox',{name:/^当前成员/})).toBeDisabled();
 await expect(page.getByRole('button',{name:'保存修改'})).toBeDisabled();
 await expect(page.getByRole('combobox',{name:/^当前成员/})).toBeEnabled({timeout:15000});
 await expect(page.getByRole('button',{name:'保存修改'})).toBeEnabled();
});

test('a failed image stays separate from valid previews and can be retried',async({page})=>{
 const invalid=fileURLToPath(new URL('../../../crates/wardrobe-core/tests/fixtures/over-limit-header.png',import.meta.url));
 const second=fileURLToPath(new URL('../../../design/studio/assets/jacket.png',import.meta.url));
 await page.goto('/');await page.getByRole('article').first().getByRole('button',{name:'查看详情'}).click();
 await openPhotoTab(page);
 await addPhotos(page,[paths[0],invalid,second]);
 await expect(page.getByRole('img',{name:'待采用的衣物展示图'})).toHaveCount(2);await expect(page.getByRole('alert')).toContainText('60 MP');await page.getByRole('button',{name:'重试此图片'}).click();await expect(page.getByRole('button',{name:'重试此图片'})).toBeEnabled();await expect(page.getByRole('img',{name:'待采用的衣物展示图'})).toHaveCount(2);
});
