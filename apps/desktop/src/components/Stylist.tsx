import {api} from '../api';import {message} from '../ItemEditor';import './Stylist.css';import {useEffect,useState} from 'react';

type Receipt={member_id:string;job_id:string;state:string};
type Job={id:string;state:string;progress:number;next_due?:number;error:{code:string;message:string}|null;result:{schema_version:number;member_id:string;facts:{call_id:number;path:string;value:unknown}[];tool_calls:{call_id:number;operation:string;arguments:Record<string,unknown>;result:Record<string,unknown>}[];budget:{model_turns:number;tool_calls:number};explanation:string;explanation_verified:boolean}|null};
const stateLabels:Record<string,string>={'queued':'已排队','running':'运行中','waiting_input':'等待核对','succeeded':'已完成','failed':'失败','cancelled':'已取消'};
const stateLabel=(state:string)=>stateLabels[state]??state;

export function Stylist({member,disabled}:{member:string;disabled:boolean}){
 const[question,setQuestion]=useState('');const[zone,setZone]=useState(()=>{try{return Intl.DateTimeFormat().resolvedOptions().timeZone;}catch{return 'UTC';}});
 const[providers,setProviders]=useState<{id:string;name:string}[]>([]),[provider,setProvider]=useState('');
 const[busy,setBusy]=useState(false),[error,setError]=useState(''),[status,setStatus]=useState('');
 const[job,setJob]=useState<Job|null>(null);
 useEffect(()=>{void loadProviders();},[member]);
 async function loadProviders(){try{const p=await api<{items:{id:string;name:string;capabilities:string[]}[]}>('provider.list',{});const text:{id:string;name:string}[]=p.items.filter((v:{id:string;name:string;capabilities:string[]})=>v.capabilities.includes('text'));setProviders(text);setProvider((current:string)=>current||(text[0]?.id??''));}catch(err){setError(message(err));}}
 async function drive(id:string){for(let round=0;round<40;round++){const current=await api<Job>('stylist.worker.run',{member_id:member,id});setJob(current);if(!['queued','running'].includes(current.state))return;}}
 async function ask(){setBusy(true);setError('');setStatus('');setJob(null);try{const receipt=await api<Receipt>('stylist.ask',{member_id:member,operation_key:crypto.randomUUID(),provider_id:provider,question:question.trim(),timezone:zone});setStatus(`任务已排队（${receipt.job_id.slice(-8)}），正在调用已配置的模型…`);await drive(receipt.job_id);}catch(err){setError(message(err));}finally{setBusy(false);setStatus('');}}
 return <section id="stylist" className="stylist"><header><div><p className="studio-eyebrow">ASK YOUR WARDROBE</p><h2>问问你的衣橱</h2><p>用一句话查询库存、搭配今天、分析衣橱；明确说「记一下今天穿了…」「这件洗过了」也可以记录实际穿着与洗护。回答中的编号和数字都来自真实查询，未知会明确说未知。</p></div><button className="secondary" disabled={disabled||busy} onClick={()=>void loadProviders()}>刷新模型列表</button></header>
 {error&&<p role="alert">{error}</p>}
  {providers.length===0&&<p role="status">还没有可用的文本模型。先到「设置 → 模型与连接」添加一个兼容 OpenAI 接口的文本模型。</p>}
 <form onSubmit={e=>{e.preventDefault();if(question.trim())void ask();}}>
  <label className="stylist-model">使用的模型<select value={provider} disabled={disabled||busy||providers.length===0} onChange={e=>setProvider(e.target.value)}><option value="">请选择已配置文本模型</option>{providers.map((p:{id:string;name:string})=><option key={p.id} value={p.id}>{p.name}</option>)}</select></label>
  <label className="stylist-question">想问什么<textarea required minLength={1} maxLength={4000} rows={3} value={question} disabled={disabled||busy||providers.length===0} placeholder="例如：今天适合穿什么？帮我看衣橱里有什么可搭的。" onChange={e=>setQuestion(e.target.value)}/></label>
  <label className="stylist-zone">时区<input required value={zone} disabled={disabled||busy} onChange={e=>setZone(e.target.value)}/></label>
  <button disabled={disabled||busy||!question.trim()||!provider} aria-busy={busy}>{busy?'正在整理回答…':'提问'} <span aria-hidden="true">↗</span></button>
 </form>
 {status&&<p role="status">{status}</p>}
 {job&&<div className="stylist-result">
  <p role="status">任务 {job.id.slice(-8)} · {stateLabel(job.state)}{job.state==='failed'&&job.error?` · ${job.error.message}`:''}{job.state==='waiting_input'?' · 上次提交结果未知，不会盲目重发；请稍后重试或检查模型服务。':''}</p>
  {job.result&&<>
   <p className="stylist-explanation">回答（未经事实核验的解释）：{job.result.explanation}</p>
   {job.result.facts.length>0&&<details open><summary>回答引用的真实数据（{job.result.facts.length} 条）</summary><ul className="stylist-facts">{job.result.facts.map((fact:{call_id:number;path:string;value:unknown},index:number)=><li key={index}><code>{fact.path}</code><span>{String(fact.value)}</span></li>)}</ul></details>}
   <details><summary>工具调用过程（{job.result.tool_calls.length} 次，均来自真实查询）</summary><ul className="stylist-tools">{job.result.tool_calls.map((call:{call_id:number;operation:string;arguments:Record<string,unknown>;result:Record<string,unknown>})=><li key={call.call_id}><strong>{call.operation}</strong><pre>{JSON.stringify(call.result,null,1)}</pre></li>)}</ul></details>
   <p className="stylist-budget">本次预算：模型 {job.result.budget.model_turns} 次 · 工具 {job.result.tool_calls.length} 次</p>
  </>}
 </div>}
 </section>;
}
