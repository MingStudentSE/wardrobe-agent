import React,{useEffect,useState} from 'react';
import {api} from '../api';
type Job={id:string;kind:string;state:string;progress?:number;error?:{message?:string}};
const states:Record<string,string>={queued:'等待处理',running:'处理中',succeeded:'已完成',failed:'失败',cancelled:'已取消'};
const kinds:Record<string,string>={text_recommendation:'搭配推荐',learning_recompute:'偏好学习',product_display:'商品展示图',image_edit:'图片处理',recognition:'衣物识别',product:'商品展示图',flatlay:'搭配展示图',tryon:'虚拟试穿',imports:'批量录入',recommendation:'搭配推荐'};
export function TaskCenter({member}:{member:string}){
 const[jobs,setJobs]=useState<Job[]>([]),[loading,setLoading]=useState(true),[error,setError]=useState('');
 useEffect(()=>{let active=true;async function load(){try{const result=await api<{items:Job[]}>('jobs.list',{member_id:member});if(active){setJobs(result.items);setError('');}}catch(e){if(active)setError(typeof e==='object'&&e&&'message'in e?String(e.message):'无法读取任务，请重试。');}finally{if(active)setLoading(false);}}void load();const timer=setInterval(()=>void load(),5000);return()=>{active=false;clearInterval(timer);};},[member]);
 async function refresh(){setLoading(true);try{const r=await api<{items:Job[]}>('jobs.list',{member_id:member});setJobs(r.items);setError('');}catch{setError('无法读取任务，请重试。');}finally{setLoading(false)}}
 return <section className="task-center"><header><div><h2>任务中心</h2><p className="helper">查看当前衣橱的识别、生成和录入任务。具体执行与重试在对应功能页进行。</p></div><button className="secondary" disabled={loading} onClick={()=>void refresh()}>刷新状态</button></header>{loading&&<p role="status">正在读取任务…</p>}{error&&<p className="feedback error" role="alert">{error}</p>}{!loading&&!jobs.length&&<div className="empty"><h3>目前没有后台任务</h3><p>识别衣物、制作商品图或生成搭配后，任务会出现在这里。</p></div>}<div className="task-list">{jobs.map(job=><article key={job.id}><div><h3>{kinds[job.kind]??'衣橱任务'}</h3><p className="helper">{states[job.state]??job.state}{job.state==='running'&&job.progress!=null?` · ${job.progress}%`:''}</p>{job.error?.message&&<p className="feedback error">{job.error.message}</p>}</div><details><summary>任务编号</summary><code>{job.id}</code></details></article>)}</div></section>
}
