import {test,expect,type Page} from '@playwright/test';
import {fileURLToPath} from 'node:url';
const paths=[fileURLToPath(new URL('../../../crates/wardrobe-core/tests/fixtures/generated-colors.png',import.meta.url)),fileURLToPath(new URL('../../../crates/wardrobe-core/tests/fixtures/generated-colors.heic',import.meta.url))];
async function picker(page:Page,selection:string[]){await page.addInitScript(paths=>{(window as any).__pickedFiles=paths;},selection);await page.route('**/*plugin-dialog*',route=>route.fulfill({contentType:'text/javascript',body:'export async function open(){return window.__pickedFiles;}'}));}
test('real images preview adopt reorder detach and retain references after editing',async({page})=>{
 await picker(page,paths);await page.goto('/');await page.getByRole('article').first().getByRole('button',{name:'查看详情'}).click();await page.getByRole('button',{name:'选择本地图片'}).click();await expect(page.getByRole('img',{name:'待采用的衣物展示图'})).toHaveCount(2);
 await page.getByRole('button',{name:'采用为主图'}).first().click();await expect(page.getByRole('status').filter({hasText:'展示图已采用'})).toContainText('展示图已采用');await page.getByRole('button',{name:'采用为主图'}).last().click();await expect(page.getByRole('status').filter({hasText:'展示图已采用'})).toContainText('展示图已采用');
 const gallery=page.getByRole('region',{name:'已关联图片'});await expect(gallery.getByRole('img')).toHaveCount(2);await gallery.getByRole('button',{name:'向前移动'}).last().click();await expect(gallery.getByRole('status').filter({hasText:'排序已保存'})).toContainText('排序已保存');
 await page.getByLabel('名称',{exact:true}).fill('图片引用验收');await page.getByRole('button',{name:'保存修改'}).click();await expect(gallery.getByRole('img')).toHaveCount(2);await gallery.getByRole('button',{name:'移除关联'}).first().click();await page.getByRole('button',{name:'确认移除关联'}).click();await expect(gallery.getByRole('img')).toHaveCount(1);
 await page.reload();await page.getByRole('article').filter({hasText:'图片引用验收'}).getByRole('button',{name:'查看详情'}).click();await expect(page.getByRole('region',{name:'已关联图片'}).getByRole('img')).toHaveCount(1);
});

test('image controls remain accessible at all specified widths',async({page})=>{
 await picker(page,paths);await page.goto('/');await page.getByRole('article').first().getByRole('button',{name:'查看详情'}).click();await page.getByRole('button',{name:'选择本地图片'}).click();await expect(page.getByRole('img',{name:'待采用的衣物展示图'})).toHaveCount(2);await page.getByRole('button',{name:'采用为主图'}).first().click();await expect(page.getByRole('region',{name:'已关联图片'}).getByRole('img').first()).toBeVisible();
 for(const width of [320,375,414,768,1024,1280,1440]) {await page.setViewportSize({width,height:900});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBeTruthy();await expect(page.getByRole('button',{name:'选择本地图片'})).toBeVisible();await page.screenshot({path:`/tmp/wardrobe-media-ui-${width}.png`,fullPage:true});await page.getByRole('region',{name:'单品图片',exact:true}).screenshot({path:`/tmp/wardrobe-media-detail-${width}.png`});}
});
test('pending native picker blocks member switching and simultaneous save',async({page})=>{
 await page.addInitScript(()=>{(window as any).__pickedFiles=[];});await page.route('**/*plugin-dialog*',route=>route.fulfill({contentType:'text/javascript',body:'export async function open(){await new Promise(resolve=>setTimeout(resolve,1000));return window.__pickedFiles;}'}));
 await page.goto('/');await page.getByRole('article').first().getByRole('button',{name:'查看详情'}).click();await page.getByRole('button',{name:'选择本地图片'}).click();await expect(page.getByRole('combobox',{name:/^当前成员/})).toBeDisabled();await expect(page.getByRole('button',{name:'保存修改'})).toBeDisabled();await expect(page.getByRole('button',{name:'选择本地图片'})).toBeEnabled();await expect(page.getByRole('combobox',{name:/^当前成员/})).toBeEnabled();
});

test('a failed image stays separate from valid previews and can be retried',async({page})=>{
 const invalid=fileURLToPath(new URL('../../../crates/wardrobe-core/tests/fixtures/over-limit-header.png',import.meta.url));await picker(page,[paths[0],invalid,paths[1]]);await page.goto('/');await page.getByRole('article').first().getByRole('button',{name:'查看详情'}).click();await page.getByRole('button',{name:'选择本地图片'}).click();await expect(page.getByRole('img',{name:'待采用的衣物展示图'})).toHaveCount(2);await expect(page.getByRole('alert')).toContainText('60 MP');await page.getByRole('button',{name:'重试此图片'}).click();await expect(page.getByRole('button',{name:'重试此图片'})).toBeEnabled();await expect(page.getByRole('img',{name:'待采用的衣物展示图'})).toHaveCount(2);
});
