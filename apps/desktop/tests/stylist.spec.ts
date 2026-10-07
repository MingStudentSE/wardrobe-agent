import {test,expect} from '@playwright/test';
import {createServer,IncomingMessage,ServerResponse,Server} from 'node:http';
// Ticket-22 GUI acceptance: the Stylist tab drives the real Application API
// through the development bridge (no mock): queued receipt, then the local
// worker loop over a scripted localhost model fixture, then a grounded answer
// citing server-extracted facts. The scripted model is the established
// controlled local HTTP oracle — no paid calls, no credentials.

let fixture:Server;let fixturePort=0;let posts=0;
const requests:string[]=[];
const replies=[
 {kind:'tool',operation:'clothing.list',arguments:{member_id:'MEMBER',limit:24}},
 {kind:'answer',facts:[{call_id:0,path:'/items/0/id'}],explanation:'以上是当前可用衣物'},
];
function startFixture():Promise<number>{return new Promise(resolve=>{
 fixture=createServer((req:IncomingMessage,res:ServerResponse)=>{
  let body='';req.on('data',chunk=>{body+=chunk;});
  req.on('end',()=>{
   const index=posts++;requests.push(body);
   const payload=JSON.stringify({choices:[{message:{content:JSON.stringify(replies[Math.min(index,replies.length-1)])}}]});
   res.setHeader('Content-Type','application/json');
   res.setHeader('Content-Length',Buffer.byteLength(payload));
   res.end(payload);
  });
 });
 fixture.listen(0,'127.0.0.1',()=>{fixturePort=(fixture.address() as {port:number}).port;resolve(fixturePort);});
});}

test.beforeAll(async()=>{await startFixture();});
test.afterAll(async()=>{fixture?.close();});

async function api(operation:string,input:object):Promise<{result?:any;error?:any}>{
 const response=await page!.request.post('/__wardrobe_dev_api',{data:{operation,input}});
 return response.json() as Promise<{result?:any;error?:any}>;
}
let page:import('@playwright/test').Page|null=null;

test('stylist tab completes a grounded tool loop through the real bridge',async({page:browserPage})=>{
 page=browserPage;
 // Seed through the real public operations, like any trusted local user.
 const space=await api('space.default',{});
 const member=space.result.member_id as string;
 // The fixture must cite this workspace's actual member id — the "default"
 // alias resolves against the persisted selection, which earlier specs in a
 // shared dev server may have changed.
 replies[0].arguments.member_id=member;
 await api('clothing.create',{member_id:member,name:'羊毛毛衣',category:'top'});
 await api('clothing.create',{member_id:member,name:'直筒牛仔裤',category:'bottom'});
 const provider=await api('provider.save',{name:'stylist spec fixture',endpoint:`http://127.0.0.1:${fixturePort}/v1`,model:'fixture-text',capabilities:['text'],authentication:'none'});
 expect(provider.result.id).toBeTruthy();
 expect(posts).toBe(0);

 await browserPage.goto('/');
 await browserPage.getByRole('button',{name:'穿搭',exact:true}).click();
 await browserPage.getByRole('button',{name:'问问衣橱',exact:true}).click();
 const panel=browserPage.locator('#stylist');
 await expect(panel).toBeVisible();
 await panel.getByLabel('使用的模型').selectOption({label:'stylist spec fixture'});
 await panel.getByLabel('想问什么').fill('今天穿什么');
 // The timezone field defaults to the machine IANA zone; keep it.
 await panel.getByRole('button',{name:'提问'}).click();
 // The bridge runs the real worker synchronously; the answer must ground in
 // the actual inventory, never a fabricated id.
 await expect(panel.getByText('任务').first()).toContainText('已完成', {timeout:30000});
 await expect(panel.getByText('以上是当前可用衣物')).toBeVisible();
 // The grounding fact must be a real id from THIS member's inventory — the
 // shared dev server may hold items seeded by earlier specs on the same member.
 const factValue=await panel.locator('.stylist-facts li span').first().textContent();
 const inventory=await api('clothing.list',{member_id:member,limit:24});
 const inventoryIds=(inventory.result.items as {id:string}[]).map(v=>v.id);
 expect(inventoryIds).toContain(factValue);
 await expect(panel.locator('.stylist-budget')).toContainText('工具 1 次');
 // Model contract: one POST per turn, and the follow-up carries the real item id.
 expect(posts).toBe(2);
 expect(requests[0]).toContain('今天穿什么');
 expect(requests[1]).toContain(factValue);
 // Read-only: no wear events recorded as a side effect.
 const wears=await api('wear.list',{member_id:member});
 expect(wears.result.items).toEqual([]);
 // The durable job replays without new model traffic.
 const jobs=await api('jobs.list',{member_id:member});
 const stylistJob=jobs.result.items.find((j:{kind:string})=>j.kind==='stylist_query');
 expect(stylistJob.state).toBe('succeeded');
 expect(posts).toBe(2);
});
