import React,{useEffect,useState} from 'react';
import {api} from '../api';
type Member={member_id:string;name:string};
export function MemberSwitcher({member,onChange,onBusy,disabled=false}:{member:string;onChange:(id:string)=>Promise<void>;disabled?:boolean;onBusy?:(busy:boolean)=>void}){
 const [members,setMembers]=useState<Member[]>([]),[name,setName]=useState(''),[newName,setNewName]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState(''),[success,setSuccess]=useState('');
 async function refresh(){const result=await api<{items:Member[]}>('member.list');setMembers(result.items);return result.items;}
 useEffect(()=>{void refresh().catch(e=>setError(String(e.message)));},[]);
 useEffect(()=>{setName(members.find(item=>item.member_id===member)?.name??'');},[member,members]);
 async function action(work:()=>Promise<void>){if(busy||disabled)return;setBusy(true);onBusy?.(true);setError('');setSuccess('');try{await work();}catch(e){setError(typeof e==='object'&&e!==null&&'message'in e?String(e.message):'无法保存成员，请重试。');}finally{setBusy(false);onBusy?.(false);}}
 async function select(id:string){await api('member.select',{member_id:id});await onChange(id);}
 return <section className="member-switcher" aria-label="成员档案" aria-busy={busy}>
 <label>当前成员<select value={member} disabled={busy||disabled||!members.length} onChange={e=>void action(()=>select(e.target.value))}>{members.map(item=><option key={item.member_id} value={item.member_id}>{item.name}</option>)}</select></label>
 <details><summary>管理成员档案</summary><form onSubmit={e=>{e.preventDefault();void action(async()=>{await api('member.update',{member_id:member,name});await refresh();setSuccess('成员名称已保存。');});}}><label>成员名称<input required maxLength={80} value={name} disabled={busy||disabled} onChange={e=>setName(e.target.value)}/></label><button className="secondary" disabled={busy||disabled||!name.trim()}>保存成员名称</button></form>
 <form onSubmit={e=>{e.preventDefault();void action(async()=>{const item=await api<Member>('member.create',{name:newName});await refresh();await select(item.member_id);setNewName('');setSuccess('已创建并切换成员。');});}}><label>新成员名称<input required maxLength={80} value={newName} disabled={busy||disabled} onChange={e=>setNewName(e.target.value)}/></label><button disabled={busy||disabled||!newName.trim()}>创建成员</button></form></details>
 {busy&&<p role="status">正在保存成员…</p>}{error&&<p role="alert" className="feedback error">{error}</p>}{success&&<p role="status" className="feedback success">{success}</p>}
 </section>;
}
