import type{Page,APIRequestContext} from '@playwright/test';
// 共享 dev server 上「当前成员」是持久选择：切换成员的规格结束后必须恢复
// 默认成员，否则同 worker 的后续规格（假设默认成员视图）会被污染。
// 这是仓库既有惯例（purchase/imports/feedback/learning/insights 的 afterEach）。
export async function restoreDefaultMember(context:APIRequestContext|Page){
 const request='post' in context&&typeof (context as APIRequestContext).post==='function'?context as APIRequestContext:(context as Page).request;
 const body=await(await request.post('/__wardrobe_dev_api',{data:{operation:'space.default',input:{}}}));
 if(body.status()>=400)return;
 const space=await body.json();
 const id=space?.result?.member_id;
 if(!id)return;
 await request.post('/__wardrobe_dev_api',{data:{operation:'member.select',input:{member_id:id}}});
}
