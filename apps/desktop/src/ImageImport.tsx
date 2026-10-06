import {useState} from 'react';
import {open} from '@tauri-apps/plugin-dialog';
import {api} from './api';
import {ImagePreview} from './ImagePreview';
type Imported={id:string;display:{bytes:number;width:number;height:number;soft_threshold_exceeded:boolean}};
type Entry={path:string;image?:Imported;error?:string};
export function ImageImport({memberId,clothingId,onAdopted}:{memberId:string;clothingId:string;onAdopted:()=>void}){
 const [entries,setEntries]=useState<Entry[]>([]),[busy,setBusy]=useState(false),[notice,setNotice]=useState('');
 async function choose(){setNotice('');try{const result=await open({multiple:true,filters:[{name:'衣物图片',extensions:['jpg','jpeg','png','webp','heic','heif']}]});if(!result)return;const paths=Array.isArray(result)?result:[result];if(paths.length>100){setNotice('每批最多选择 100 张图片。');return;}setBusy(true);setEntries(paths.map(path=>({path})));for(let i=0;i<paths.length;i++){try{const image=await api<Imported>('assets.import',{member_id:memberId,path:paths[i]});setEntries(old=>old.map((v,n)=>n===i?{...v,image}:v));}catch(e){setEntries(old=>old.map((v,n)=>n===i?{...v,error:message(e)}:v));}}}catch(e){setNotice(message(e));}finally{setBusy(false);}}
 async function adopt(id:string){setBusy(true);try{await api('assets.adopt',{member_id:memberId,clothing_id:clothingId,asset_id:id});setNotice('展示图已采用，原图保留在本设备。');onAdopted();}catch(e){setNotice(message(e));}finally{setBusy(false);}}
 return <section className="image-import" aria-label="单品图片"><h3>单品图片</h3><p className="helper">原图仅保留在导入设备。选择图片后，先预览压缩展示图再采用。</p><button type="button" className="secondary" onClick={()=>void choose()} disabled={busy} aria-busy={busy}>{busy?'正在处理图片…':'选择本地图片'}</button><ol>{entries.map((entry,i)=><li key={`${entry.path}-${i}`}><p>{entry.path.split(/[\\/]/).pop()}</p>{entry.error?<p role="alert">{entry.error}</p>:entry.image?<><ImagePreview memberId={memberId} assetId={entry.image.id} alt="待采用的衣物展示图" thumbnail={false}/><p>{entry.image.display.width} × {entry.image.display.height} · {Math.round(entry.image.display.bytes/1024)} KB</p>{entry.image.display.soft_threshold_exceeded&&<p>图片超过 500 KB，已保留细节；采用前请检查纹理。</p>}<button type="button" disabled={busy} onClick={()=>void adopt(entry.image!.id)}>采用为主图</button></>:<p role="status">等待处理…</p>}</li>)}</ol>{notice&&<p role="status">{notice}</p>}</section>;
}
function message(e:unknown){return typeof e==='object'&&e!==null&&'message'in e?String(e.message):'图片处理失败，请重试。';}
