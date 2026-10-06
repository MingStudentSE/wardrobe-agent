import React, { useEffect, useState, useRef } from 'react';
import { createRoot } from 'react-dom/client';
import { invoke } from '@tauri-apps/api/core';
import './styles.css';
type Clothing={id:string;name:string;category:string;seasons:string[];purchase_price_minor:number|null;price_exponent:number|null;currency:string|null};
const categories:Record<string,string>={top:'上装',bottom:'下装',outerwear:'外套',dress:'连衣裙',one_piece:'连体',shoes:'鞋',bag:'包',accessory:'配饰',underwear:'内衣',sportswear:'运动服',other:'其他'};
const seasons:Record<string,string>={spring:'春',summer:'夏',autumn:'秋',winter:'冬'};
async function api<T>(operation:string,input:object={}):Promise<T>{
 if (import.meta.env.DEV && !('__TAURI_INTERNALS__' in window)) { const response=await fetch('/__wardrobe_dev_api',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({operation,input})}); const body=await response.json();if(body.error)throw body.error;return body.result; }
 return invoke('application',{operation,input});
}
function moneyExponent(currency:string){try{return new Intl.NumberFormat('zh-CN',{style:'currency',currency}).resolvedOptions().maximumFractionDigits??2;}catch{return 2;}}
function moneyMinor(price:string,currency:string){
 const exponent=moneyExponent(currency),[whole,fraction='']=price.split('.');
 if(!/^\d+$/.test(whole)||!/^[0-9]*$/.test(fraction)||fraction.length>exponent)throw new Error(`该币种价格最多保留 ${exponent} 位小数，请修改价格。`);
 const minor=Number(whole+fraction.padEnd(exponent,'0'));
 if(!Number.isSafeInteger(minor))throw new Error('价格超出可录入范围，请检查金额。');
 return minor;
}
function App(){
 const [items,setItems]=useState<Clothing[]>([]),[member,setMember]=useState(''),[cursor,setCursor]=useState<string|null>(null),[loading,setLoading]=useState(true),[saving,setSaving]=useState(false),[error,setError]=useState(''),[success,setSuccess]=useState(''),[name,setName]=useState(''),[category,setCategory]=useState('top'),[selected,setSelected]=useState<string[]>([]),[price,setPrice]=useState(''),[currency,setCurrency]=useState('CNY');
 const [pageLoading,setPageLoading]=useState(false),[refreshing,setRefreshing]=useState(false);
 const replacementPending=useRef(false),mutationPending=useRef(false);
 const generation=useRef(0),pageRequest=useRef<number|null>(null),sequence=useRef(0);
 async function list(id:string,after?:string){
  if(after&&(pageRequest.current!==null||replacementPending.current||mutationPending.current))return;
  const epoch=after?generation.current:++generation.current;
  const request=++sequence.current;
  if(after){pageRequest.current=request;setPageLoading(true);}else{replacementPending.current=true;setRefreshing(true);pageRequest.current=null;setPageLoading(false);}
  try{
   const result=await api<{items:Clothing[];next_cursor:string|null}>('clothing.list',{member_id:id,limit:24,...(after?{cursor:after}:{})});
   if(epoch!==generation.current)return;
   setItems(old=>after?[...old,...result.items.filter(item=>!old.some(existing=>existing.id===item.id))]:result.items);setCursor(result.next_cursor);
  }finally{if(pageRequest.current===request){pageRequest.current=null;setPageLoading(false);}if(!after&&epoch===generation.current){replacementPending.current=false;setRefreshing(false);}}
 }
 async function start(){setLoading(true);setError('');try{const space=await api<{member_id:string}>('space.default');setMember(space.member_id);await list(space.member_id);}catch(e){setError(message(e));}finally{setLoading(false);}}
 useEffect(()=>{void start();},[]);
 function message(e:unknown){return typeof e==='object'&&e!==null&&'message'in e?String(e.message):'无法读取本地衣橱，请重试。';}
 async function save(e:React.FormEvent){e.preventDefault();if(mutationPending.current)return;mutationPending.current=true;setError('');setSuccess('');setSaving(true);try{await api('clothing.create',{member_id:member,name,category,seasons:selected,purchase_price_minor:price===''?null:moneyMinor(price,currency),currency:price===''?null:currency});setName('');setPrice('');setSelected([]);await list(member);setSuccess('已保存到本地衣橱。');}catch(e){setError(message(e));}finally{mutationPending.current=false;setSaving(false);}}
 return <div className="workbench"><aside className="rail"><p className="brand">衣橱 / WARDROBE</p><p className="rail-label">本地空间</p><h1>我的衣橱</h1><a href="#collection" aria-current="page">单品</a><p className="rail-note">从一件衣服开始。<br/>无需模型配置，随时保存。</p></aside><main><header><div><p className="eyebrow">我的收藏</p><h2 id="collection">衣物，慢慢整理。</h2><p>记录已有的衣服，让下一次选择更轻松。</p></div><a className="button secondary" href="#entry">录入单品</a></header><div className="workspace"><section className="collection" aria-label="衣物列表" aria-busy={loading}>{loading?<p role="status">正在打开本地衣橱…</p>:items.length===0?<div className="empty"><span aria-hidden="true">＋</span><h3>这里等待你的第一件衣服</h3><p>没有图片也可以保存，未知信息可以留空。</p><a href="#entry">填写单品资料</a></div>:<><div className="grid">{items.map(item=><article key={item.id}><div className="no-image" aria-label="暂无图片">{categories[item.category]}<span>暂无图片</span></div><h3>{item.name}</h3><p>{item.seasons.length?item.seasons.map(s=>seasons[s]).join(' · '):'季节未设置'}</p><p>{item.purchase_price_minor===null?'价格未知':`${(item.purchase_price_minor/10**(item.price_exponent??2)).toFixed(item.price_exponent??2)} ${item.currency}`}</p></article>)}</div>{cursor&&<button className="secondary" disabled={pageLoading||saving||loading||refreshing} aria-busy={pageLoading||refreshing} onClick={()=>void list(member,cursor).catch(e=>setError(message(e)))}>{pageLoading?'正在加载…':'加载更多'}</button>}</>}</section><section className="entry" id="entry"><p className="eyebrow">手工录入</p><h2>添一件单品</h2><form onSubmit={save}><label>名称<input required maxLength={120} value={name} onChange={e=>{setName(e.target.value);setSuccess('');}} placeholder="例如：白色棉衬衫"/></label><label>类别<select value={category} onChange={e=>setCategory(e.target.value)}>{Object.entries(categories).map(([value,label])=><option key={value} value={value}>{label}</option>)}</select></label><fieldset><legend>适用季节 <span>可留空</span></legend><div className="season-row">{Object.entries(seasons).map(([value,label])=><label key={value}><input type="checkbox" checked={selected.includes(value)} onChange={e=>setSelected(old=>e.target.checked?[...old,value]:old.filter(v=>v!==value))}/>{label}</label>)}</div></fieldset><div className="price-row"><label>购入价格<input type="number" min="0" step={10**-moneyExponent(currency)} value={price} onChange={e=>setPrice(e.target.value)} placeholder="未知可留空"/></label><label>币种<input pattern="[A-Z]{3}" value={currency} onChange={e=>setCurrency(e.target.value)} maxLength={3}/></label></div><p className="helper">价格可留空，未知信息无需填写。</p><button disabled={saving||loading||!member} aria-busy={saving}>{saving?'正在保存…':'保存单品'}</button>{error&&<div className="feedback error" role="alert"><p>{error}</p><button type="button" className="secondary" onClick={()=>void start()}>重新读取</button></div>}{success&&<p className="feedback success" role="status">✓ {success}</p>}</form></section></div></main></div>;
}
createRoot(document.getElementById('root')!).render(<App/>);
