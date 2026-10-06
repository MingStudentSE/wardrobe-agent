import React, { useEffect, useState, useRef } from 'react';
import { createRoot } from 'react-dom/client';
import { api } from './api';
import './styles.css';
import {Insights} from './components/Insights';
import {WearHistory,WearIntent} from './components/WearHistory';
import {Lookbook} from './components/Lookbook';
import {BatchImport} from './BatchImport';
import {EncryptionSettings} from './components/EncryptionSettings';
import {ClientSettings} from './components/ClientSettings';
import {ModelSettings} from './components/ModelSettings';
import {MemberSwitcher} from './components/MemberSwitcher';
import {ItemEditor,Clothing,message} from './ItemEditor';
import {Collection,Filters,initialFilters,filterInput} from './Collection';
function App(){
 const [items,setItems]=useState<Clothing[]>([]),[member,setMember]=useState(''),[cursor,setCursor]=useState<string|null>(null),[loading,setLoading]=useState(true),[saving,setSaving]=useState(false),[error,setError]=useState('');
 const [filters,setFilters]=useState(initialFilters),[editing,setEditing]=useState<Clothing|null>(null),[editorKey,setEditorKey]=useState(0);
 const [wearIntent,setWearIntent]=useState<WearIntent|null>(null);
 const [memberBusy,setMemberBusy]=useState(false);
 const memberPending=useRef(false);
 const filtersRef=useRef(initialFilters);
 const [pageLoading,setPageLoading]=useState(false),[refreshing,setRefreshing]=useState(false);
 const replacementPending=useRef(false),mutationPending=useRef(false);
 const generation=useRef(0),pageRequest=useRef<number|null>(null),sequence=useRef(0);
 async function list(id:string,after?:string){
  if(after&&(pageRequest.current!==null||replacementPending.current||mutationPending.current||memberPending.current))return;
  const epoch=after?generation.current:++generation.current;
  const request=++sequence.current;
  if(after){pageRequest.current=request;setPageLoading(true);}else{replacementPending.current=true;setRefreshing(true);pageRequest.current=null;setPageLoading(false);}
  try{
   const result=await api<{items:Clothing[];next_cursor:string|null}>('clothing.list',{member_id:id,limit:24,...filterInput(filtersRef.current),...(after?{cursor:after}:{})});
   if(epoch!==generation.current)return;
   setItems(old=>after?[...old,...result.items.filter(item=>!old.some(existing=>existing.id===item.id))]:result.items);setCursor(result.next_cursor);
  }finally{if(pageRequest.current===request){pageRequest.current=null;setPageLoading(false);}if(!after&&epoch===generation.current){replacementPending.current=false;setRefreshing(false);}}
 }
 async function start(){setLoading(true);setError('');try{const space=await api<{member_id:string;selected_member_id?:string}>('space.default');const id=space.selected_member_id??space.member_id;setMember(id);await list(id);}catch(e){setError(message(e));}finally{setLoading(false);}}
 useEffect(()=>{void start();},[]);
 function beginWear(intent:WearIntent){if(memberPending.current||mutationPending.current)return;setWearIntent(intent);document.getElementById('wears')?.scrollIntoView({block:'start'});}
 function closeEditor(){setEditing(null);setEditorKey(old=>old+1);}
 function changeFilters(next:Filters){filtersRef.current=next;setFilters(next);setError('');void list(member).catch(e=>setError(message(e)));}
 return <div className="workbench"><aside className="rail"><p className="brand">衣橱 / WARDROBE</p><p className="rail-label">本地空间</p><h1>我的衣橱</h1><MemberSwitcher member={member} disabled={saving||loading} onBusy={busy=>{memberPending.current=busy;setMemberBusy(busy);}} onChange={async id=>{setMember(id);setWearIntent(null);setItems([]);setCursor(null);closeEditor();setError('');await list(id);}}/><a href="#collection" aria-current="page">单品</a><a href="#lookbook">搭配</a><a href="#wears">穿着</a><a href="#insights">洞察</a><p className="rail-note">整理已有衣物。<br/>手工录入随时可用。</p></aside><main><header><div><h2 id="collection">衣物，慢慢整理。</h2><p>衣橱、归档与回收站，保留每件衣物的事实。</p></div><a className="button secondary" href="#entry" onClick={e=>{if(saving||memberBusy){e.preventDefault();return;}closeEditor();}}>录入单品</a></header>{error&&<div className="feedback error" role="alert"><p>{error}</p><button className="secondary" onClick={()=>void start()}>重新读取</button></div>}<div className="workspace"><Collection items={items} filters={filters} onFilters={changeFilters} onSelect={item=>{setEditing(item);setEditorKey(old=>old+1);document.getElementById('entry')?.scrollIntoView({block:'start'});}} loading={loading} cursor={cursor} pageLoading={pageLoading} disabled={saving||loading||refreshing||memberBusy} onMore={()=>{if(cursor)void list(member,cursor).catch(e=>setError(message(e)));}}/><ItemEditor onWear={beginWear} key={editorKey} member={member} item={editing} disabled={loading||memberBusy} canMutate={()=>!memberPending.current&&!mutationPending.current} onSaved={async()=>{await list(member);}} onClose={closeEditor} onBusy={busy=>{mutationPending.current=busy;setSaving(busy);}}/></div><Lookbook onWear={beginWear} key={member} member={member} disabled={loading||memberBusy||saving} canMutate={()=>!memberPending.current&&!mutationPending.current} onBusy={busy=>{mutationPending.current=busy;setSaving(busy);}}/><WearHistory key={`wear-${member}`} member={member} intent={wearIntent} disabled={loading||memberBusy||saving} canMutate={()=>!memberPending.current&&!mutationPending.current} onBusy={busy=>{mutationPending.current=busy;setSaving(busy);}}/><Insights key={`insights-${member}`} member={member} disabled={loading||memberBusy||saving} canMutate={()=>!memberPending.current&&!mutationPending.current} onBusy={busy=>{mutationPending.current=busy;setSaving(busy);}}/><BatchImport member={member} disabled={loading||memberBusy} canMutate={()=>!memberPending.current&&!mutationPending.current} onSaved={async()=>{await list(member);}} onBusy={busy=>{mutationPending.current=busy;setSaving(busy);}}/><ModelSettings/><ClientSettings/><EncryptionSettings/></main></div>;
}
createRoot(document.getElementById('root')!).render(<App/>);
