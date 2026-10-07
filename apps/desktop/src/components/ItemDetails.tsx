import {useCategories} from '../useCategories';
import {useRef,useState} from 'react';
import {Clothing,seasons,subLabels,colorLabels} from '../ItemEditor';
import {ImagePreview} from '../ImagePreview';
import './item-details.css';
export function ItemDetails({item,onEdit,onWear,disabled,onDelete}:{item:Clothing;onEdit:()=>void;onWear:()=>void;disabled:boolean;onDelete:()=>Promise<void>}){
 const categories=useCategories(item.member_id);
 const [confirmDelete,setConfirmDelete]=useState(false),[deleting,setDeleting]=useState(false),[deleteError,setDeleteError]=useState('');const pending=useRef(false);
 async function remove(){if(pending.current||disabled)return;pending.current=true;setDeleting(true);setDeleteError('');try{await onDelete();}catch(e){setDeleteError(e instanceof Error?e.message:'删除失败，请重试。');}finally{pending.current=false;setDeleting(false);}}
 const hasPhoto=typeof item.display_asset_id==='string';
 const color=typeof item.color_description==='string'&&item.color_description.trim()?item.color_description:Array.isArray(item.color)?item.color.map(v=>colorLabels[String(v)]||String(v)).join('、'):'';
 const facts=[['品类',categories[item.category]],['小类',typeof item.sub_category==='string'?(subLabels[item.sub_category]||item.sub_category):null],['颜色',color],['品牌',item.brand],['适用季节',item.seasons.length?item.seasons.map(s=>seasons[s]).join(' · '):'不限季节'],['尺寸',item.size_label]].filter(([,v])=>typeof v==='string'&&v);
 return <section className={`item-details garment-detail ${hasPhoto?'with-photo':'without-photo'}`} aria-label="单品详情">
 <div className="garment-detail-body">
 {hasPhoto&&<div className="garment-detail-image"><ImagePreview memberId={item.member_id} assetId={String(item.display_asset_id)} alt={item.name} thumbnail={false}/></div>}
 <div className="garment-detail-info"><div className="garment-detail-meta"><span className="garment-state">{item.status==='deleted'?'回收站':item.status==='archived'?'已归档':'在衣橱'}</span>{item.favorite&&<span className="garment-favorite">已收藏</span>}</div><h3>{item.name}</h3><p className="garment-subtitle">{[categories[item.category],typeof item.brand==='string'?item.brand:null].filter(Boolean).join(' / ')}</p>
 <dl className="garment-facts">{facts.map(([label,value])=><div key={String(label)}><dt>{String(label)}</dt><dd>{String(value)}</dd></div>)}</dl>
 {typeof item.notes==='string'&&item.notes.trim()&&<div className="garment-note"><h4>备注</h4><p>{item.notes}</p></div>}
 {!hasPhoto&&<div className="garment-photo-empty"><svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5"><rect x="3" y="4" width="18" height="16" rx="3"/><circle cx="9" cy="9" r="1.5"/><path d="m4 17 5-5 4 4 3-3 5 5"/></svg><div><strong>还没有单品照片</strong><span>添加照片，让衣橱更好辨认。</span></div><button type="button" className="secondary" disabled={disabled} onClick={onEdit}>添加照片</button></div>}
 </div></div>
 {confirmDelete&&<div className="garment-delete-confirm" role="group" aria-label="确认删除衣物"><strong>将“{item.name}”移入回收站？</strong><p>可在回收站恢复，照片和历史记录会保留。</p>{deleteError&&<p role="alert">{deleteError}</p>}<div><button type="button" className="secondary" disabled={deleting} onClick={()=>setConfirmDelete(false)}>取消删除</button><button type="button" className="danger" disabled={disabled||deleting} aria-busy={deleting} onClick={()=>void remove()}>{deleting?'正在删除…':'确认删除'}</button></div></div>}
 <footer className="garment-detail-footer">{item.status!=='deleted'&&<button type="button" className="secondary garment-delete" disabled={disabled||deleting} onClick={()=>setConfirmDelete(true)}>删除衣物</button>}<button type="button" className="garment-history secondary" disabled={disabled} onClick={onEdit}>照片与历史<span aria-hidden="true"> ↗</span></button><div><button type="button" className="secondary" disabled={disabled} onClick={onEdit}>{item.status==='deleted'?'恢复与历史':'编辑衣物'}</button>{item.status!=='deleted'&&<button type="button" disabled={disabled} onClick={onWear}>今天穿了</button>}</div></footer>
 </section>;
}
