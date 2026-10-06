import {useEffect,useState} from 'react';
import {api} from './api';
export function ImagePreview({memberId,assetId,alt,thumbnail=true}:{memberId:string;assetId:string;alt:string;thumbnail?:boolean}){
 const [url,setUrl]=useState(''),[error,setError]=useState(false),[attempt,setAttempt]=useState(0);
 useEffect(()=>{let active=true;setUrl('');setError(false);void api<{data_url:string}>('assets.preview',{member_id:memberId,id:assetId,variant:thumbnail?'thumbnail':'display'}).then(v=>{if(active)setUrl(v.data_url);}).catch(()=>{if(active)setError(true);});return()=>{active=false;};},[memberId,assetId,thumbnail,attempt]);
 return url?<img className="asset-preview" src={url} alt={alt} onError={()=>{setUrl('');setError(true);}}/>:error?<div className="image-error"><p role="alert">图片暂不可读取</p><button type="button" className="secondary" onClick={()=>setAttempt(v=>v+1)}>重新读取图片</button></div>:<p role="status">正在读取图片…</p>;
}
