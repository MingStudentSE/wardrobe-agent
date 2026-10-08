import {api} from '../api';

// 诊断导出的共用交互约定（#47 诊断页与 #49 错误反馈条同源）：
// - Tauri：系统保存对话框，默认文件名 衣橱日志-YYYYMMDD-HHmm.jsonl，用户取消则不导出；
// - dev/e2e 回退：相对路径 exports/衣橱日志-YYYYMMDD-HHmm.jsonl，由开发桥按 WARDROBE_DATA_DIR 展开并 mkdir。
export type ExportResult={path:string;event_count:number;bytes:number;covered_from:string|null;covered_to:string|null};
export type ExportScope='session'|'recent'|'all';
export const isTauri=()=>typeof window!=='undefined'&&'__TAURI_INTERNALS__' in window;
export function formatBytes(bytes:number){if(bytes<1024)return `${bytes} B`;if(bytes<1024*1024)return `${(bytes/1024).toFixed(1)} KB`;return `${(bytes/1024/1024).toFixed(1)} MB`;}
function stamp(){const now=new Date(),pad=(value:number)=>String(value).padStart(2,'0');return `${now.getFullYear()}${pad(now.getMonth()+1)}${pad(now.getDate())}-${pad(now.getHours())}${pad(now.getMinutes())}`;}
async function pickExportTarget(){
 if(isTauri()){
  const {save}=await import('@tauri-apps/plugin-dialog');
  const picked=await save({defaultPath:`衣橱日志-${stamp()}.jsonl`,filters:[{name:'诊断文件',extensions:['jsonl']}]});
  return picked??null;
 }
 return `exports/衣橱日志-${stamp()}.jsonl`;
}
export async function exportDiagnostics(range:{scope:ExportScope}|{from:string;to:string}):Promise<ExportResult|null>{
 const path=await pickExportTarget();
 if(!path)return null;
 const input:Record<string,unknown>='scope' in range?{path,scope:range.scope}:{path,from:range.from,to:range.to};
 return api<ExportResult>('diagnostics.export',input);
}
export async function revealExportedFile(path:string){
 const {revealItemInDir}=await import('@tauri-apps/plugin-opener');
 await revealItemInDir(path);
}
