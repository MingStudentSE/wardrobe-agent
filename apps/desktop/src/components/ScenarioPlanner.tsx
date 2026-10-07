import {api} from '../api';import {message} from '../ItemEditor';import './ScenarioPlanner.css';import {useState} from 'react';

// 场景与旅行计划（issue #23）：婚礼/面试单事件与多日旅行计划、去重打包清单。
// 计划是领域工件（creates_wear 恒为 false），结构性不记录实际穿着；
// 保存的穿搭模板走 outfit.create，同样不产生穿着。

type PlanItem={clothing_id:string;name:string;category:string;role:string;layer:number};
type PlanDay={index:number;date:string;complete:boolean;missing_roles:string[];items:PlanItem[];reasons:string[]};
type PackingItem={clothing_id:string;name:string;category:string;role:string;days:number[];reused:boolean;consecutive_repeat:boolean};
type Plan={schema_version:number;id:string;member_id:string;kind:'event'|'travel';title:string;occasion:string;status:'active'|'deleted';version:number;start_date:string;days_count:number;timezone:string;controls:{title?:string;notes?:string;weather?:{mode:string;temperature_c?:number};fixed_ids?:string[];excluded_ids?:string[]};days:PlanDay[];packing:{total:number;items:PackingItem[]};creates_wear:boolean;shortage_reasons:string[];generation:{source:string;attempts:number;failure_codes?:string[];model_feedback?:string;provider_requests?:number};weather_context:{mode?:string;source?:string;temperature_c?:number|null;stale?:boolean;unavailable_reason?:string|null;last_error?:{message:string}|null};notes?:string};
type Receipt={schema_version:number;member_id:string;job_id:string;state:string;plan_id?:string};
type Job={id:string;state:string;result?:{plan_id?:string}|null;error?:{code:string;message:string}|null};
type Stock={clothing_id:string;name:string;available:boolean;exclusion_reasons:string[]};
const roleLabels:Record<string,string>={top:'上装',bottom:'下装',body:'连体衣物',shoes:'鞋',outerwear:'外层',bag:'包',accessory:'配饰',underwear:'内衣',sportswear:'运动服',other:'其他'};
const garmentCategories=new Set(['top','bottom','dress','one_piece','sportswear','underwear']);
const weekdayLabels=['周日','周一','周二','周三','周四','周五','周六'];
const roleLabel=(role:string)=>roleLabels[role]??role;
function dayLabel(date:string){const parsed=new Date(`${date}T12:00:00`);return Number.isNaN(parsed.getTime())?date:`${date} ${weekdayLabels[parsed.getDay()]??''}`;}

export function ScenarioPlanner({member,disabled}:{member:string;disabled:boolean}){
 const[plan,setPlan]=useState<Plan|null>(null),[plans,setPlans]=useState<Plan[]>([]);
 const[kind,setKind]=useState<'event'|'travel'>('event'),[occasion,setOccasion]=useState('wedding'),[days,setDays]=useState('3'),[title,setTitle]=useState(''),[notes,setNotes]=useState('');
 const[startDate,setStartDate]=useState(()=>new Date().toISOString().slice(0,10)),[zone,setZone]=useState(()=>{try{return Intl.DateTimeFormat().resolvedOptions().timeZone;}catch{return 'UTC';}});
 const[weather,setWeather]=useState('off'),[temperature,setTemperature]=useState('20');
 const[strategy,setStrategy]=useState('rules'),[providers,setProviders]=useState<{id:string;name:string}[]>([]),[provider,setProvider]=useState('');
 const[stock,setStock]=useState<Stock[]>([]),[fixed,setFixed]=useState<string[]>([]),[excluded,setExcluded]=useState<string[]>([]);
 const[updateNotes,setUpdateNotes]=useState(''),[updateFixed,setUpdateFixed]=useState<string[]>([]),[updateExcluded,setUpdateExcluded]=useState<string[]>([]);
 const[busy,setBusy]=useState(false),[error,setError]=useState(''),[status,setStatus]=useState(''),[confirmDelete,setConfirmDelete]=useState(false);
 function toggle(id:string,ids:string[],set:(next:string[])=>void){set(ids.includes(id)?ids.filter(value=>value!==id):[...ids,id]);}
 async function load(){try{const[list,stock,models]=await Promise.all([api<{items:Plan[]}>('scenario.list',{member_id:member}),api<{items:Stock[]}>('care.availability',{member_id:member}),api<{items:{id:string;name:string;capabilities:string[]}[]}>('provider.list',{})]);setPlans(list.items);setStock(stock.items);setProviders(models.items.filter(v=>v.capabilities.includes('text')));}catch(err){setError(message(err));}}
 async function show(id:string){try{setError('');setPlan(await api<Plan>('scenario.get',{member_id:member,id}));setUpdateNotes('');setUpdateFixed([]);setUpdateExcluded([]);}catch(err){setError(message(err));}}
 async function receive(value:Plan|Receipt){if('days' in value){setPlan(value);return;}
  const id=value.job_id;
  for(let round=0;round<120;round++){const job=await api<Job>('jobs.get',{member_id:member,id});void api<Job>('scenario.worker.run',{member_id:member,id}).catch(()=>undefined);
   if(job.result?.plan_id){await show(job.result.plan_id);setStatus('');return;}
   if(['failed','cancelled'].includes(job.state)){throw new Error(job.error?.message??'场景任务没有可采用结果，请查看任务状态。');}
   if(job.state==='waiting_input'){throw new Error(job.error?.message??'模型提交结果未知，已停止等待核对；不会盲重发。');}
   await new Promise(resolve=>setTimeout(resolve,300));}
  throw new Error('场景任务等待超时，请稍后在任务中心查看。');}
 async function generate(){setBusy(true);setError('');setStatus('');try{const input:Record<string,unknown>={member_id:member,operation_key:crypto.randomUUID(),timezone:zone,kind,title,...(kind==='event'?{occasion}:{days:Number(days)}),start_date:startDate,fixed_ids:fixed,excluded_ids:excluded,strategy};
  if(notes.trim())input.notes=notes.trim();
  if(weather!=='off')input.weather=weather==='automatic'?{mode:'automatic'}:{mode:'manual',temperature_c:Number(temperature)};
  if(strategy==='ai')input.provider_id=provider;
  setStatus(strategy==='ai'?'已排队场景任务，正在调用已配置的模型…':'正在基于真实库存生成计划…');
  await receive(await api<Plan|Receipt>('scenario.plan',input));
  await load();setStatus('计划已生成并保存；计划不记录实际穿着。');}catch(err){setError(message(err));setStatus('');}finally{setBusy(false);}}
 async function applyUpdate(){if(!plan)return;setBusy(true);setError('');try{const input:Record<string,unknown>={member_id:member,id:plan.id,expected_version:plan.version};
  if(updateNotes.trim())input.notes=updateNotes.trim();
  if(updateExcluded.length)input.excluded_ids=updateExcluded;
  if(updateFixed.length)input.fixed_ids=updateFixed;
  if(weather!=='off')input.weather=weather==='automatic'?{mode:'automatic'}:{mode:'manual',temperature_c:Number(temperature)};
  setPlan(await api<Plan>('scenario.update',input));setUpdateNotes('');setUpdateFixed([]);setUpdateExcluded([]);setStatus('计划已按新约束重算保存。');}catch(err){setError(message(err));}finally{setBusy(false);}}
 async function saveAsOutfit(day:PlanDay){if(!plan)return;setBusy(true);setError('');try{const name=`${plan.title} · 第${day.index+1}天`;await api('outfit.create',{member_id:member,name,items:day.items.map(item=>({clothing_id:item.clothing_id,role:item.role,layer:item.layer}))});window.dispatchEvent(new Event('wardrobe-outfits-changed'));setStatus('已保存为穿搭模板；模板不记录实际穿着。');}catch(err){setError(message(err));}finally{setBusy(false);}}
 async function removePlan(){if(!plan)return;setBusy(true);setError('');try{await api('scenario.delete',{member_id:member,id:plan.id});setPlan(null);await load();setStatus('计划已移入回收站。');}catch(err){setError(message(err));}finally{setBusy(false);}}

 return <section id="scenarios" className="scenarios"><header><div><p className="studio-eyebrow">SCENARIOS & TRAVEL</p><h2>场景与旅行计划</h2><p>婚礼、面试与多日旅行：基于真实库存逐天搭配并生成去重打包清单。计划与打包清单只是安排，不会记成实际穿着；库存不足会如实说明原因。</p></div><button className="secondary" disabled={busy} onClick={()=>void load()}>刷新计划列表</button></header>
 {error&&<p role="alert">{error}</p>}
 {status&&<p role="status">{status}</p>}
 <form className="scenario-form" onSubmit={e=>{e.preventDefault();void generate();}}>
  <fieldset><legend>场景类型</legend><div className="scenario-chips" role="group" aria-label="场景类型">{([['event','婚礼/面试'],['travel','多日旅行']] as const).map(([value,label])=><button type="button" key={value} className="secondary" aria-pressed={kind===value} disabled={busy||disabled} onClick={()=>setKind(value)}>{label}</button>)}</div></fieldset>
  {kind==='event'&&<fieldset><legend>场合</legend><div className="scenario-chips" role="group" aria-label="场合">{([['wedding','婚礼'],['interview','面试']] as const).map(([value,label])=><button type="button" key={value} className="secondary" aria-pressed={occasion===value} disabled={busy||disabled} onClick={()=>setOccasion(value)}>{label}</button>)}</div></fieldset>}
  <div className="scenario-fields">
   {kind==='travel'&&<label>旅行天数<input type="number" min={2} max={14} required value={days} disabled={busy||disabled} onChange={e=>setDays(e.target.value)}/></label>}
   <label>开始日期<input type="date" required value={startDate} disabled={busy||disabled} onChange={e=>setStartDate(e.target.value)}/></label>
   <label>标题（可选）<input maxLength={120} placeholder={kind==='travel'?'旅行计划':'婚礼计划'} value={title} disabled={busy||disabled} onChange={e=>setTitle(e.target.value)}/></label>
   <label>时区<input required value={zone} disabled={busy||disabled} onChange={e=>setZone(e.target.value)}/></label>
  </div>
  <label>手工说明（可选）<textarea rows={2} maxLength={1000} placeholder="例如：婚礼在室内，按室温准备" value={notes} disabled={busy||disabled} onChange={e=>setNotes(e.target.value)}/></label>
  <fieldset><legend>天气参考（可选；失败不阻断）</legend><div className="scenario-fields">
   <label>天气<select value={weather} disabled={busy||disabled} onChange={e=>setWeather(e.target.value)}><option value="off">不参考天气</option><option value="manual">手动温度</option><option value="automatic">已配置地点天气</option></select></label>
   {weather==='manual'&&<label>手动温度 °C<input type="number" min={-80} max={60} required value={temperature} disabled={busy||disabled} onChange={e=>setTemperature(e.target.value)}/></label>}
  </div></fieldset>
  <details className="scenario-items"><summary>指定想穿/不想穿的单品 <span>固定 {fixed.length} · 排除 {excluded.length}</span></summary><ul className="scenario-stock">{stock.map(item=><li key={item.clothing_id}><strong>{item.name}</strong>{!item.available&&<span className="scenario-unavailable">不可用 · {item.exclusion_reasons.join(' / ')}</span>}<span className="scenario-stock-actions"><label><input type="checkbox" aria-label={`固定 ${item.name}`} disabled={busy||disabled||!item.available} checked={fixed.includes(item.clothing_id)} onChange={()=>toggle(item.clothing_id,fixed,setFixed)}/>固定</label><label><input type="checkbox" aria-label={`排除 ${item.name}`} disabled={busy||disabled} checked={excluded.includes(item.clothing_id)} onChange={()=>toggle(item.clothing_id,excluded,setExcluded)}/>排除</label></span></li>)}</ul></details>
  <fieldset><legend>生成方式</legend><div className="scenario-fields">
   <div className="scenario-chips" role="group" aria-label="生成方式">{([['rules','本地规则'],['ai','模型辅助']] as const).map(([value,label])=><button type="button" key={value} className="secondary" aria-pressed={strategy===value} disabled={busy||disabled} onClick={()=>setStrategy(value)}>{label}</button>)}</div>
   {strategy==='ai'&&<label>使用模型<select required value={provider} disabled={busy||disabled||providers.length===0} onChange={e=>setProvider(e.target.value)}><option value="">请选择已配置文本模型</option>{providers.map(p=><option key={p.id} value={p.id}>{p.name}</option>)}</select>{providers.length===0&&<span>先在「设置 → 模型与连接」添加文本模型。</span>}</label>}
  </div></fieldset>
  <button disabled={busy||disabled||(strategy==='ai'&&!provider)} aria-busy={busy}>{busy?'正在安排…':'生成计划'} <span aria-hidden="true">↗</span></button>
  <p className="scenario-note">计划与打包清单不会记成实际穿着；模型辅助失败时会按本地规则降级并明确提示。</p>
 </form>
 {plan&&<div className="scenario-result">
  <div className="scenario-plan-head"><h3>{plan.title} <small>{plan.kind==='travel'?`${plan.days_count} 天旅行`:'单事件'} · v{plan.version} · {plan.generation.source==='text_model'?'模型方案（已通过服务端校验）':plan.generation.source==='rule_fallback'?'模型失败，已按本地规则降级':'本地规则方案'}</small></h3>{confirmDelete?<span className="scenario-stock-actions"><button type="button" disabled={busy} onClick={()=>void removePlan()}>确认删除</button><button type="button" className="secondary" disabled={busy} onClick={()=>setConfirmDelete(false)}>取消</button></span>:<button type="button" className="secondary" disabled={busy} onClick={()=>setConfirmDelete(true)}>删除计划</button>}</div>
  {plan.notes&&<p className="scenario-note">手工说明：{plan.notes}</p>}
  {plan.generation.source==='rule_fallback'&&<p role="status" className="scenario-feedback">模型反馈：{plan.generation.model_feedback??'模型输出未通过校验，已按本地规则生成。'}{plan.generation.failure_codes?.length?`（失败码：${plan.generation.failure_codes.join(' / ')}）`:''}</p>}
  {plan.shortage_reasons.length>0&&<div role="alert" className="scenario-shortage"><strong>库存与约束说明</strong><ul>{plan.shortage_reasons.map(reason=><li key={reason}>{reason}</li>)}</ul></div>}
  <div className="scenario-days">{plan.days.map(day=><article key={day.index} className="scenario-day"><header><h4>第 {day.index+1} 天 · {dayLabel(day.date)}</h4><span className={day.complete?'scenario-ok':'scenario-missing'}>{day.complete?'方案完整':`缺少：${day.missing_roles.map(roleLabel).join(' / ')}`}</span></header>
   <ul className="scenario-day-items">{day.items.map(item=><li key={item.clothing_id}><strong>{item.name}</strong><span>{roleLabel(item.role)}</span></li>)}</ul>
   <details><summary>这一天的决策说明</summary><ul>{day.reasons.map(reason=><li key={reason}>{reason}</li>)}</ul></details>
   <button type="button" className="secondary" disabled={busy||disabled||!day.complete} onClick={()=>void saveAsOutfit(day)}>保存这一天为穿搭模板</button>
  </article>)}</div>
  <section className="scenario-packing"><h4>去重打包清单（{plan.packing.total} 件）</h4><p className="scenario-note">同一衣物只出现一次；「第 n 天」列出实际使用的日子，不重复计数。</p>
   <ul className="scenario-packing-list">{plan.packing.items.map(item=><li key={item.clothing_id}><strong>{item.name}</strong><span>{roleLabel(item.role)} · 第 {item.days.map(value=>value+1).join('、')} 天</span>{item.consecutive_repeat&&garmentCategories.has(item.category)?<em>连续两天穿着（库存不足）</em>:item.reused?<em>旅行中重复使用</em>:null}</li>)}</ul>
  </section>
  <details className="scenario-update"><summary>修改计划（按新约束重算，保存不记穿着）</summary>
   <label>补充手工说明<textarea rows={2} maxLength={1000} value={updateNotes} disabled={busy||disabled} placeholder="例如：第二天下雨，加入雨具说明" onChange={e=>setUpdateNotes(e.target.value)}/></label>
   <fieldset><legend>调整固定/排除（排除会自动解除同单品的固定）</legend><ul className="scenario-stock">{stock.map(item=><li key={item.clothing_id}><strong>{item.name}</strong><span className="scenario-stock-actions"><label><input type="checkbox" aria-label={`修改时固定 ${item.name}`} disabled={busy||disabled||!item.available} checked={updateFixed.includes(item.clothing_id)} onChange={()=>toggle(item.clothing_id,updateFixed,setUpdateFixed)}/>固定</label><label><input type="checkbox" aria-label={`修改时排除 ${item.name}`} disabled={busy||disabled} checked={updateExcluded.includes(item.clothing_id)} onChange={()=>toggle(item.clothing_id,updateExcluded,setUpdateExcluded)}/>排除</label></span></li>)}</ul></fieldset>
   <button type="button" disabled={busy||disabled} onClick={()=>void applyUpdate()}>按新约束重算并保存</button>
  </details>
 </div>}
 <section className="scenario-archive"><h4>已保存的计划</h4>{plans.length===0?<p>还没有已保存的场景计划。</p>:<ul className="scenario-plan-list">{plans.map(item=><li key={item.id}><span>{item.title} · {item.kind==='travel'?`${item.days_count} 天`:'单事件'} · {item.start_date}</span><button type="button" className="secondary" disabled={busy} onClick={()=>void show(item.id)}>查看</button></li>)}</ul>}</section>
 </section>;
}
export type {Plan as ScenarioPlan};
