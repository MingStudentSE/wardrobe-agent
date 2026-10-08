import {test,expect} from '@playwright/test';
import {restoreDefaultMember} from './support/member-hygiene';
import {createServer,IncomingMessage,ServerResponse,Server} from 'node:http';
// Ticket-22 GUI acceptance: the Stylist tab drives the real Application API
// through the development bridge (no mock): queued receipt, then the local
// worker loop over a scripted localhost model fixture, then a grounded answer
// citing server-extracted facts. Write coverage: an explicit user instruction
// records an actual wear through the controlled write tool, with the server
// owning the idempotency key and calendar date, and the durable read-back.

let fixture:Server;let fixturePort=0;
let posts=0;
const requests:string[]=[];
// The active script is swapped per test; workers=1 keeps tests sequential.
let script:{kind:string;operation?:string;arguments?:Record<string,unknown>;facts?:{call_id:number;path:string}[];explanation:string}[]=[
 {kind:'tool',operation:'clothing.list',arguments:{member_id:'MEMBER',limit:24}},
 {kind:'answer',facts:[{call_id:0,path:'/items/0/id'}],explanation:'以上是当前可用衣物'},
];
function startFixture():Promise<number>{return new Promise(resolve=>{
 fixture=createServer((req:IncomingMessage,res:ServerResponse)=>{
  let body='';req.on('data',chunk=>{body+=chunk;});
  req.on('end',()=>{
   const index=posts++;requests.push(body);
   const turn=script[Math.min(index,script.length-1)];
   const payload=JSON.stringify({choices:[{message:{content:JSON.stringify(turn)}}]});
   res.setHeader('Content-Type','application/json');
   res.setHeader('Content-Length',Buffer.byteLength(payload));
   res.end(payload);
  });
 });
 fixture.listen(0,'127.0.0.1',()=>{fixturePort=(fixture.address() as {port:number}).port;resolve(fixturePort);});
});}

test.beforeEach(async({request})=>{await restoreDefaultMember(request);});
test.beforeAll(async()=>{await startFixture();});
test.afterAll(async()=>{fixture?.close();});

let page:import('@playwright/test').Page|null=null;
async function api(operation:string,input:object):Promise<{result?:any;error?:any}>{
 const response=await page!.request.post('/__wardrobe_dev_api',{data:{operation,input}});
 return response.json() as Promise<{result?:any;error?:any}>;
}
async function ask(page:any,question:string){
 await page.goto('/');
 await page.getByRole('button',{name:'穿搭',exact:true}).click();
 await page.getByRole('button',{name:'问问衣橱',exact:true}).click();
 const panel=page.locator('#stylist');
 await expect(panel).toBeVisible();
 await panel.getByLabel('使用的模型').selectOption({label:'stylist spec fixture'});
 await panel.getByLabel('想问什么').fill(question);
 await panel.getByRole('button',{name:'提问'}).click();
 return panel;
}

test('stylist tab completes a grounded tool loop through the real bridge',async({page:browserPage})=>{
 page=browserPage;posts=0;requests.length=0;
 // Seed through the real public operations, like any trusted local user.
 const space=await api('space.default',{});
 const member=space.result.member_id as string;
 // The fixture must cite this workspace's actual member id — the "default"
 // alias resolves against the persisted selection, which earlier specs in a
 // shared dev server may have changed.
 script=[
  {kind:'tool',operation:'clothing.list',arguments:{member_id:member,limit:24}},
  {kind:'answer',facts:[{call_id:0,path:'/items/0/id'}],explanation:'以上是当前可用衣物'},
 ];
 await api('clothing.create',{member_id:member,name:'羊毛毛衣',category:'top'});
 await api('clothing.create',{member_id:member,name:'直筒牛仔裤',category:'bottom'});
 const provider=await api('provider.save',{name:'stylist spec fixture',endpoint:`http://127.0.0.1:${fixturePort}/v1`,model:'fixture-text',capabilities:['text'],authentication:'none'});
 expect(provider.result.id).toBeTruthy();
 expect(posts).toBe(0);

 const panel=await ask(browserPage,'今天穿什么');
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

test('explicit instruction records an actual wear with server-owned idempotency',async({page:browserPage})=>{
 page=browserPage;posts=0;requests.length=0;
 const space=await api('space.default',{});
 const member=space.result.member_id as string;
 const item=await api('clothing.create',{member_id:member,name:'风衣',category:'outerwear'});
 const itemId=item.result.id as string;
 // The model maps the explicit instruction to the write tool; the server
 // strips the forged key and date, injecting its own member/key/today.
 script=[
  {kind:'tool',operation:'wear.record',arguments:{member_id:member,clothing_ids:[itemId],notes:'今天穿了风衣',operation_key:'forged-key',local_date:'1999-01-01'}},
  {kind:'answer',facts:[{call_id:0,path:'/id'}],explanation:'已记录今天的穿着'},
 ];
 const panel=await ask(browserPage,'记一下今天穿了风衣');
 await expect(panel.getByText('任务').first()).toContainText('已完成', {timeout:30000});
 await expect(panel.getByText('已记录今天的穿着')).toBeVisible();
 // The ledger exposes the write with server-owned fields.
 const calls=await api('jobs.list',{member_id:member});
 const writeJob=calls.result.items.filter((j:{kind:string})=>j.kind==='stylist_query')
  .sort((a:{created_at:string},b:{created_at:string})=>(a.created_at<b.created_at?1:-1))[0];
 const run=await api('stylist.worker.run',{member_id:member,id:writeJob.id});
 const toolCall=run.result.result.tool_calls[0];
 expect(toolCall.operation).toBe('wear.record');
 expect(String(toolCall.arguments.operation_key)).toMatch(/^stylist-/);
 expect(toolCall.arguments.operation_key).not.toBe('forged-key');
 expect(toolCall.arguments.local_date).not.toBe('1999-01-01');
 expect(toolCall.arguments.clothing_ids).toEqual([itemId]);
 // Durable read-back: exactly one actual wear event for this member, citing
 // the seeded item and the server-injected calendar date (today in the
 // timezone the GUI sent — not UTC); the fact value is the real event id.
 const wears=await api('wear.list',{member_id:member});
 const events=(wears.result.items as {id:string;kind:string;local_date:string;items:{clothing_id:string}[]}[]);
 expect(events.length).toBe(1);
 expect(events[0].kind).toBe('record');
 expect(events[0].local_date).toBe(toolCall.arguments.local_date);
 expect(events[0].items.map(v=>v.clothing_id)).toContain(itemId);
 const factValue=await panel.locator('.stylist-facts li span').first().textContent();
 expect(events[0].id).toBe(factValue);
 expect(posts).toBe(2);
});
