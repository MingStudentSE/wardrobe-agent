import {formatBytes,isTauri,revealExportedFile,type ExportResult} from './diagnostics-export';

// 导出结果展示（复查收拢）：诊断页「导出日志」「记录复现」与错误条「导出日志」三处
// 同一形状——「已导出 N 个事件，共 …」+ 文件路径 + 打开所在文件夹（仅 Tauri）。
// 行为与 aria 不变：role=status、类名 diagnostics-export-result 沿用既有样式。
export function ExportResultNotice({result,heading='已导出'}:{result:ExportResult;heading?:string}){
 return <div role="status" className="diagnostics-export-result">
  <p>{heading} {result.event_count} 个事件，共 {formatBytes(result.bytes)}。</p>
  <p><code>{result.path}</code></p>
  {isTauri()&&<div><button className="secondary" onClick={()=>void revealExportedFile(result.path)}>打开所在文件夹</button></div>}
 </div>;
}
