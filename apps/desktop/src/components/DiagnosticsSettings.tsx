/* 诊断 · 设置第 9 张卡：健康摘要、基础检查、最近错误与链路展开、运行会话、记录复现、导出。
 * 与 CLI doctor / diagnostics.* 同源；遵循既有 token，无 hover/拖拽依赖，键盘可达。 */
import React,{useCallback,useEffect,useRef,useState} from 'react';
import {api,platformMode} from '../api';
import {isTauriRuntime} from '../platform';
import './DiagnosticsSettings.css';

type Check={id:string;status:'pass'|'warn'|'fail';message:string;hint:string;detail:Record<string,unknown>};
type Checks={schema_version:number;items:Check[];generated_at:string};
type LogEvent={ts:string;level:string;kind:string;run_id:string;trace_id?:string;op:string;result:string;duration_ms?:number;detail?:Record<string,unknown>};
type Summary={schema_version:number;run_id:string;database:Record<string,unknown>;jobs?:Record<string,number>;provider_unknown?:number|null;logs:{total_bytes:number;files:number;max_total_bytes:number;retain_days:number;dropped:number};recent_errors:LogEvent[]};
type ExportResult={path:string;events:number;bytes:number;from:string;to:string;kind:string};

const checkLabels:Record<string,string>={data_directory:'数据目录',database:'数据库',database_wal:'写入方式（WAL）',logs:'日志记录',disk_space:'磁盘空间',installation_generation:'本机安装信息',encryption:'加密状态',model_credentials:'模型凭据',stuck_jobs:'后台任务',pending_move_marker:'数据位置切换'};
const statusGlyph:Record<Check['status'],string>={pass:'✓',warn:'!',fail:'✕'};
const targetLinks:Record<string,{label:string;tab:string}>={data_directory:{label:'前往 备份与恢复',tab:'backup'},database:{label:'前往 备份与恢复',tab:'backup'},disk_space:{label:'前往 备份与恢复',tab:'backup'},model_credentials:{label:'前往 模型与连接',tab:'model'},stuck_jobs:{label:'前往 任务中心',tab:'tasks'},pending_move_marker:{label:'前往 备份与恢复',tab:'backup'},logs:{label:'前往 备份与恢复',tab:'backup'},encryption:{label:'前往 加密与设备',tab:'encryption'},installation_generation:{label:'前往 备份与恢复',tab:'backup'},database_wal:{label:'重新打开应用即可恢复',tab:''}};
const levelLabels:Record<string,string>={info:'信息',warn:'警告',error:'错误',debug:'调试',trace:'详细'};
const message=(e:unknown)=>e&&typeof e==='object'&&'message'in e?String((e as {message:unknown}).message):'诊断操作暂时不可用，请重试。';
function bytes(value:number){return value>1024*1024?`${(value/1024/1024).toFixed(1)} MB`:`${Math.max(1,Math.round(value/1024))} KB`;}
function shortTime(ts:string){return ts.length>=16?ts.slice(11,19):ts;}

export function DiagnosticsSettings(){
 const[summary,setSummary]=useState<Summary|null>(null);
 const[checks,setChecks]=useState<Checks|null>(null);
 const[error,setError]=useState('');
 const[notice,setNotice]=useState('');
 const[loading,setLoading]=useState(true);
 const[expandedTrace,setExpandedTrace]=useState<string|null>(null);
 const[traceEvents,setTraceEvents]=useState<LogEvent[]|null>(null);
 const[range,setRange]=useState<'recent'|'session'|'all'>('recent');
 const[exporting,setExporting]=useState(false);
 const[exportResult,setExportResult]=useState<ExportResult|null>(null);
 const[recording,setRecording]=useState<string|null>(null);
 const recordingRef=useRef(false);
 const alive=useRef(true);

 const reload=useCallback(async()=>{
  setError('');
  try{
   const[s,c]=await Promise.all([
    api<Summary>('diagnostics.summary'),
    api<Checks>('diagnostics.checks'),
   ]);
   if(alive.current){setSummary(s);setChecks(c);}
  }catch(e){if(alive.current)setError(message(e));}finally{if(alive.current)setLoading(false);}
 },[]);
 useEffect(()=>{alive.current=true;void reload();return()=>{alive.current=false;};},[reload]);

 // 复现记录最长时间到达后自动停止（与核心 10 分钟上限一致的界面提示）
 useEffect(()=>{
  if(!recording)return;
  const timer=window.setTimeout(()=>{
   if(recordingRef.current)void stopRecording(true);
  },10*60*1000);
  return()=>window.clearTimeout(timer);
 // eslint-disable-next-line react-hooks/exhaustive-deps
 },[recording]);

 async function openTrace(trace:string){
  setExpandedTrace(trace);setTraceEvents(null);
  try{
   const result=await api<{items:LogEvent[]}>(`diagnostics.logs.query`,{trace_id:trace,limit:100});
   if(alive.current)setTraceEvents(result.items);
  }catch(e){if(alive.current)setError(message(e));}
 }

 async function runExport(){
  if(exporting)return;
  setExporting(true);setError('');setNotice('');setExportResult(null);
  try{
   const input:Record<string,unknown>={range};
   let saved=input;
   if(isTauriRuntime){
    const{save}=await import('@tauri-apps/plugin-dialog');
    const path=await save({defaultPath:`衣橱日志-${new Date().toISOString().slice(0,13).replace(/[-T:]/g,'')}.jsonl`});
    if(!path){setExporting(false);return;}
    input.path=path as string;
   }
   const result=await api<ExportResult>('diagnostics.export',saved);
   if(alive.current)setExportResult(result);
  }catch(e){if(alive.current)setError(message(e));}finally{if(alive.current)setExporting(false);}
 }

 async function startRecording(){
  setError('');setNotice('');
  try{
   const started=await api<{started_at:string}>('diagnostics.reproduction.start');
   recordingRef.current=true;
   if(alive.current){setRecording(started.started_at);setNotice('正在记录复现：正常使用衣橱，完成后回到这里点「停止并导出」。');}
  }catch(e){setError(message(e));}
 }

 async function stopRecording(auto=false){
  if(!recordingRef.current)return;
  recordingRef.current=false;
  setError('');
  try{
   const result=await api<ExportResult&{auto_stopped:boolean}>('diagnostics.reproduction.stop');
   if(alive.current){setRecording(null);setExportResult(result);setNotice(auto||result.auto_stopped?'复现记录达到最长时间，已自动停止并导出。':'复现记录已停止并导出，文件可直接发给维护者。');}
  }catch(e){
   recordingRef.current=false;
   if(alive.current){setRecording(null);setError(message(e));}
  }
 }

 const failing=checks?.items.filter(item=>item.status!=='pass')??[];
 return <section className="diagnostics-settings" id="diagnostics-settings" aria-label="诊断">
  <div className="settings-content">
   <p>这里汇集衣橱的运行状况：一键体检、查看最近错误，并把可排查的日志文件发给维护者。日志独立于数据库保存，已按构造排除凭据与隐私内容。</p>
   {loading&&<p role="status">正在读取诊断信息…</p>}
   {error&&<p className="feedback error" role="alert">{error}</p>}
   {notice&&<p className="feedback success" role="status">{notice}</p>}

   <details className="settings-workflow" open>
    <summary>健康状况</summary>
    {summary&&<div className="diag-summary">
     <p>本次启动编号 <code>{summary.run_id.slice(0,8)}</code>；数据库{summary.database.matches_application===true?'结构与当前应用一致':'状态需要关注'}；日志已记录 {bytes(summary.logs.total_bytes)}（保留 {summary.logs.retain_days} 天）。</p>
     <p className="helper">失败任务 {summary.jobs?.failed??0} 项，等待处理 {summary.jobs?.waiting_input??0} 项，供应商结果未知 {summary.provider_unknown??0} 项。{summary.logs.dropped>0?`有 ${summary.logs.dropped} 条日志因磁盘问题未能写入。`:''}</p>
    </div>}
    <div className="diag-actions">
     <button className="secondary" onClick={()=>void reload()}>刷新诊断信息</button>
     <label className="diag-range">导出范围
      <select value={range} onChange={event=>setRange(event.target.value as typeof range)}>
       <option value="recent">最近 24 小时（跨重启）</option>
       <option value="session">本次启动</option>
       <option value="all">保留期内全部</option>
      </select>
     </label>
     <button disabled={exporting} onClick={()=>void runExport()}>{exporting?'正在导出…':'导出诊断文件'}</button>
    </div>
    {exportResult&&<div className="diag-export-result" role="status">
     <p>已保存 <strong>{exportResult.events}</strong> 条事件（{bytes(exportResult.bytes)}）到：</p>
     <p><code>{exportResult.path}</code></p>
     {isTauriRuntime&&<button className="secondary" onClick={()=>{void import('@tauri-apps/api/core').then(({invoke})=>invoke('reveal_path',{path:exportResult.path}).catch(()=>{}));}}>打开所在文件夹</button>}
    </div>}
   </details>

   <details className="settings-workflow" open>
    <summary>基础检查{checks?`（${checks.items.length-failing.length}/${checks.items.length} 项正常）`:''}</summary>
    {checks&&<ul className="diag-checks">
     {checks.items.map(item=><li key={item.id} data-check-id={item.id} className={`diag-check diag-check-${item.status}`}>
      <span className="diag-check-status" aria-hidden="true">{statusGlyph[item.status]}</span>
      <div>
       <p><strong>{checkLabels[item.id]??item.id}</strong> · {item.message}</p>
       {item.hint&&<p className="helper">{item.hint}{targetLinks[item.id]?.tab&&<> <button className="link-button" onClick={()=>{window.dispatchEvent(new CustomEvent('wardrobe:open-settings',{detail:targetLinks[item.id].tab}));}}>{targetLinks[item.id].label}</button></>}</p>}
      </div>
     </li>)}
    </ul>}
   </details>

   <details className="settings-workflow" open>
    <summary>最近错误</summary>
    {summary&&summary.recent_errors.length===0&&<p className="helper">近期没有错误记录。</p>}
    {summary&&summary.recent_errors.length>0&&<ul className="diag-errors">
     {summary.recent_errors.map((event,index)=><li key={`${event.ts}-${index}`} className="diag-error">
      <div className="diag-error-row">
       <span className="diag-error-time">{shortTime(event.ts)}</span>
       <span className="diag-error-op"><code>{event.op}</code></span>
       <span className="diag-error-code">{event.result}</span>
       {event.trace_id&&<button className="link-button" aria-expanded={expandedTrace===event.trace_id} onClick={()=>{if(expandedTrace===event.trace_id){setExpandedTrace(null);setTraceEvents(null);}else{void openTrace(event.trace_id as string);}}}>{expandedTrace===event.trace_id?'收起链路':'查看完整链路'}</button>}
      </div>
      {expandedTrace===event.trace_id&&<ol className="diag-trace" aria-label="链路事件序列">
       {(traceEvents??[]).map((step,stepIndex)=><li key={stepIndex} className={`diag-trace-step level-${step.level}`}>
        <span className="diag-trace-time">{shortTime(step.ts)}</span>
        <code>{step.op}</code>
        <span>{step.result==='ok'?'成功':step.result}</span>
        {step.kind==='provider'&&step.detail&&<span className="helper">{String(step.detail.model??'')} {String(step.detail.endpoint_category??'')}</span>}
       </li>)}
       {traceEvents&&!traceEvents.length&&<li className="helper">链路下暂无日志事件。</li>}
      </ol>}
     </li>)}
    </ul>}
   </details>

   <details className="settings-workflow" open>
    <summary>记录复现</summary>
    <p className="helper">点「开始记录复现」后正常使用衣橱，操作步骤会与日志一起记成时间线；完成后「停止并导出」得到一个单文件，直接发给维护者即可还原操作顺序。记录最长 10 分钟，到时自动停止。</p>
    <div className="diag-actions">
     {!recording
      ?<button onClick={()=>void startRecording()}>开始记录复现</button>
      :<button onClick={()=>void stopRecording(false)}>停止并导出</button>}
     {recording&&<p role="status" className="diag-recording">记录中……（开始于 {shortTime(recording)}）</p>}
    </div>
   </details>

   <details className="settings-workflow">
    <summary>日志文件位置</summary>
    {summary&&<p className="helper">日志按天保存在数据目录 <code>logs/</code> 子目录内，独立于数据库；单个文件与总占用有上限，过期自动清理。桌面端可点上方「打开所在文件夹」直接定位导出的文件。{platformMode!=='tauri'?' 当前为浏览器/开发环境，导出的文件保存在衣橱数据目录的 exports/ 内。':''}</p>}
   </details>
  </div>
 </section>;
}
