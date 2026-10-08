import React,{useEffect,useState} from 'react';
import {api} from '../api';
import {settingsEntries} from './SettingsHub';
import './Diagnostics.css';
type RecentError={ts:string;level:string;event_type:string;run_id:string;trace_id?:string;operation?:string;result?:string;reason?:string;message?:string};
type Summary={
 database_schema_version:number;database_user_version:number;installation_generation:number;
 current_run_id:string;
 jobs_by_state:{queued:number;running:number;waiting_input:number;succeeded:number;failed:number;cancelled:number};
 provider_outcome_unknown:number;
 recent_errors:RecentError[];
 logs:{total_bytes:number;file_count:number;retention_days:number;max_total_bytes:number;dropped_events:number};
};
type CheckItem={id:string;status:'pass'|'warn'|'fail';message:string;setting:string|null;detail:string};
type ChecksResult={schema_version:number;checks:CheckItem[];counts:{pass:number;warn:number;fail:number}};
type LogEvent={ts:string;level:string;event_type:string;run_id:string;operation?:string;duration_ms?:number;result?:string;reason?:string;message?:string};
type ExportResult={path:string;event_count:number;bytes:number;covered_from:string|null;covered_to:string|null};
type Scope='session'|'recent'|'all'|'window';
type TraceState={loading:boolean;items:LogEvent[];error?:string};
const isTauri=()=>typeof window!=='undefined'&&'__TAURI_INTERNALS__' in window;
function formatTime(ts:string){const date=new Date(ts);return isNaN(date.getTime())?ts:date.toLocaleString('zh-CN',{hour12:false,month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',second:'2-digit'});}
function formatBytes(bytes:number){if(bytes<1024)return `${bytes} B`;if(bytes<1024*1024)return `${(bytes/1024).toFixed(1)} KB`;return `${(bytes/1024/1024).toFixed(1)} MB`;}
function stamp(){const now=new Date(),pad=(value:number)=>String(value).padStart(2,'0');return `${now.getFullYear()}${pad(now.getMonth()+1)}${pad(now.getDate())}-${pad(now.getHours())}${pad(now.getMinutes())}`;}
function shortId(id:string){return id?id.slice(0,8):'—';}
function errorText(e:unknown){return typeof e==='object'&&e&&'message'in e?String((e as {message?:unknown}).message):'操作失败，请重试。';}
type Metric={label:string;value:string;note:string};
const CHECK_STATUS_TEXT:{pass:string;warn:string;fail:string}={pass:'通过',warn:'警告',fail:'失败'};
const SETTING_TITLES:Record<string,string>=Object.fromEntries(settingsEntries.map(entry=>[entry.id,entry.title]));
function CheckIcon({status}:{status:CheckItem['status']}){const paths:Record<CheckItem['status'],React.ReactNode>={
 pass:<path d="M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18Zm-4 9 3 3 5-6"/>,
 warn:<path d="M12 4 2.5 20h19L12 4Zm0 6v4m0 3v.5"/>,
 fail:<path d="M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18Zm-3 6 6 6m0-6-6 6"/>};
 return <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">{paths[status]}</svg>;}
export function DiagnosticsPanel({onNavigate}:{onNavigate?:(tab:string)=>void}){
 const [summary,setSummary]=useState<Summary|null>(null),[loading,setLoading]=useState(true),[error,setError]=useState('');
 const [checks,setChecks]=useState<ChecksResult|null>(null),[checksLoading,setChecksLoading]=useState(true),[checksError,setChecksError]=useState('');
 const [traces,setTraces]=useState<Record<string,TraceState>>({});
 const [scope,setScope]=useState<Scope>('recent'),[from,setFrom]=useState(''),[to,setTo]=useState('');
 const [exporting,setExporting]=useState(false),[exportError,setExportError]=useState(''),[exportResult,setExportResult]=useState<ExportResult|null>(null);
 const [logsDir,setLogsDir]=useState(''),[openNote,setOpenNote]=useState(''),[openError,setOpenError]=useState('');
 async function refresh(){
  setLoading(true);setError('');setChecksLoading(true);setChecksError('');
  try{const result=await api<Summary>('diagnostics.summary');setSummary(result);}catch(e){setError(errorText(e));}finally{setLoading(false);}
  // 基础检查与健康摘要同一操作源（diagnostics.checks），独立加载互不阻塞
  try{const result=await api<ChecksResult>('diagnostics.checks');setChecks(result);}catch(e){setChecksError(errorText(e));}finally{setChecksLoading(false);}
 }
 useEffect(()=>{let active=true;void refresh();void api<{directory:string}>('data_directory.status').then(status=>{if(active)setLogsDir(status.directory.replace(/\/+$/,'')+'/logs');}).catch(()=>{});return()=>{active=false;};},[]);
 async function loadTrace(traceId:string){
  setTraces(old=>({...old,[traceId]:{loading:true,items:[]}}));
  try{const result=await api<{items:LogEvent[]}>('diagnostics.logs.query',{trace_id:traceId,limit:100});setTraces(old=>({...old,[traceId]:{loading:false,items:result.items}}));}
  catch(e){setTraces(old=>({...old,[traceId]:{loading:false,items:[],error:errorText(e)}}));}
 }
 async function exportLogs(){
  setExportError('');setExportResult(null);
  if(scope==='window'&&(!from||!to)){setExportError('请选择时间窗的起止时间。');return;}
  setExporting(true);
  try{
   const name=`衣橱日志-${stamp()}.jsonl`;
   let path:string;
   if(isTauri()){
    const {save}=await import('@tauri-apps/plugin-dialog');
    const picked=await save({defaultPath:name,filters:[{name:'诊断文件',extensions:['jsonl']}]});
    if(!picked){setExporting(false);return;}
    path=picked;
   }else{
    // dev/e2e 回退：相对路径由开发桥按数据目录展开到 exports/
    path=`exports/${name}`;
   }
   const input:Record<string,unknown>=scope==='window'?{path,from:new Date(from).toISOString(),to:new Date(to).toISOString()}:{path,scope};
   setExportResult(await api<ExportResult>('diagnostics.export',input));
  }catch(e){setExportError(errorText(e));}finally{setExporting(false);}
 }
 async function revealPath(path:string){
  const {revealItemInDir}=await import('@tauri-apps/plugin-opener');
  await revealItemInDir(path);
 }
 async function openLogsDirectory(){
  setOpenError('');setOpenNote('');
  if(!logsDir){setOpenError('暂时无法定位日志目录，请重试。');return;}
  if(isTauri()){
   try{await revealPath(logsDir);setOpenNote('已在文件夹中显示日志目录。');}
   catch(e){setOpenError(errorText(e));}
  }else{
   setOpenNote(`当前是开发环境，无法打开文件夹窗口；日志目录：${logsDir}`);
  }
 }
 const metrics:Metric[]=summary?[
  {label:'数据库模式',value:`v${summary.database_schema_version}`,note:summary.database_user_version===summary.database_schema_version?'与当前应用一致':`数据库为 v${summary.database_user_version}，需重启升级`},
  {label:'运行会话',value:shortId(summary.current_run_id),note:'本次启动的运行标识'},
  {label:'失败任务',value:String(summary.jobs_by_state.failed),note:`待输入 ${summary.jobs_by_state.waiting_input} · 已完成 ${summary.jobs_by_state.succeeded}`},
  {label:'结果未知',value:String(summary.provider_outcome_unknown),note:'已提交但未拿到供应商结果的任务'},
  {label:'日志占用',value:formatBytes(summary.logs.total_bytes),note:`${summary.logs.file_count} 个文件 · 保留 ${summary.logs.retention_days} 天${summary.logs.dropped_events?` · 丢弃 ${summary.logs.dropped_events} 条`:''}`},
 ]:[];
 const errors=[...(summary?.recent_errors??[])].reverse();
 return <section className="diagnostics" id="diagnostics-settings" aria-label="诊断">
  <header className="settings-page-heading"><div><h2>诊断</h2><p>查看衣橱运行健康状况，追踪错误的完整链路，并把日志导出给维护者排查。</p></div><button className="secondary" disabled={loading} onClick={()=>void refresh()}>刷新诊断</button></header>
  {loading&&<p role="status">正在读取诊断摘要…</p>}
  {error&&<div className="feedback error" role="alert"><p>{error}</p><button className="secondary" onClick={()=>void refresh()}>重试</button></div>}
  {summary&&!loading&&<>
   <section className="diagnostics-section" aria-label="健康摘要">
    <h3>健康摘要</h3>
    <div className="diagnostics-metrics">{metrics.map(metric=><article className="diagnostics-metric" key={metric.label}><span>{metric.label}</span><strong>{metric.value}</strong><small>{metric.note}</small></article>)}</div>
    <div className="diagnostics-logs-row">
     <button className="secondary" onClick={()=>void openLogsDirectory()}>打开日志目录</button>
     {logsDir&&<code>{logsDir}</code>}
    </div>
    {openNote&&<p role="status" className="diagnostics-logs-path">{openNote}</p>}
    {openError&&<p role="alert" className="feedback error">{openError}</p>}
   </section>
   <section className="diagnostics-section" aria-label="基础检查">
    <h3>基础检查</h3>
    {checksLoading&&<p role="status">正在运行基础检查…</p>}
    {checksError&&<div className="feedback error" role="alert"><p>{checksError}</p><button className="secondary" onClick={()=>void refresh()}>重试</button></div>}
    {checks&&!checksLoading&&<>
     <p role="status" className="diagnostics-checks-summary">
      {checks.counts.fail?`${checks.counts.fail} 项失败`:checks.counts.warn?`${checks.counts.warn} 项警告`:'全部通过'} · 共 {checks.checks.length} 项体检，只诊断不改配置
     </p>
     <ul className="diagnostics-checks">
      {checks.checks.map(item=><li key={item.id} className={`diagnostics-check diagnostics-check-${item.status}`}>
       <div className="diagnostics-check-head">
        <span className="diagnostics-check-status"><CheckIcon status={item.status}/>{CHECK_STATUS_TEXT[item.status]}</span>
        <strong className="diagnostics-check-message">{item.message}</strong>
        {item.setting&&<button className="secondary diagnostics-check-goto" onClick={()=>onNavigate?.(item.setting!)}>去「{SETTING_TITLES[item.setting]??item.setting}」</button>}
       </div>
       <code className="diagnostics-check-detail">{item.detail}</code>
      </li>)}
     </ul>
    </>}
   </section>
   <section className="diagnostics-section diagnostics-export" aria-label="导出日志">
    <h3>导出日志</h3>
    <fieldset><legend>导出范围</legend>
     <label><input type="radio" name="diagnostics-export-scope" value="session" checked={scope==='session'} onChange={()=>setScope('session')}/>本次启动</label>
     <label><input type="radio" name="diagnostics-export-scope" value="recent" checked={scope==='recent'} onChange={()=>setScope('recent')}/>最近 24 小时</label>
     <label><input type="radio" name="diagnostics-export-scope" value="all" checked={scope==='all'} onChange={()=>setScope('all')}/>保留期内全部</label>
     <label><input type="radio" name="diagnostics-export-scope" value="window" checked={scope==='window'} onChange={()=>setScope('window')}/>指定时间窗</label>
    </fieldset>
    {scope==='window'&&<div className="diagnostics-window">
     <label>开始时间<input type="datetime-local" value={from} onChange={event=>setFrom(event.target.value)}/></label>
     <label>结束时间<input type="datetime-local" value={to} onChange={event=>setTo(event.target.value)}/></label>
    </div>}
    <button disabled={exporting} onClick={()=>void exportLogs()}>{exporting?'正在导出…':'导出日志文件'}</button>
    {exportError&&<p className="feedback error" role="alert">{exportError}</p>}
    {exportResult&&<div role="status" className="diagnostics-export-result">
     <p>已导出 {exportResult.event_count} 个事件，共 {formatBytes(exportResult.bytes)}。</p>
     <p><code>{exportResult.path}</code></p>
     {isTauri()&&<div><button className="secondary" onClick={()=>void revealPath(exportResult.path)}>打开所在文件夹</button></div>}
    </div>}
   </section>
   <section className="diagnostics-section" aria-label="最近错误">
    <h3>最近错误</h3>
    {!errors.length&&<div className="settings-empty"><h3>暂无错误记录</h3><p>操作失败时会出现在这里，并附上排查用的链路编号。</p></div>}
    <ul className="diagnostics-errors">{errors.map(item=>item.trace_id&&<li key={`${item.ts}-${item.trace_id}`}>
     <details className="diagnostics-error" onToggle={event=>{const target=event.currentTarget;if(target.open&&!traces[item.trace_id!])void loadTrace(item.trace_id!);}}>
      <summary>
       <span className="diagnostics-error-time">{formatTime(item.ts)}</span>
       <span className="diagnostics-error-operation">{item.operation??item.event_type}</span>
       <code className="diagnostics-error-code">{item.result??'error'}</code>
       <span className={item.run_id===summary.current_run_id?'diagnostics-session-badge current':'diagnostics-session-badge'}>{item.run_id===summary.current_run_id?'本次会话':'更早会话'}</span>
       <code className="diagnostics-trace-id">链路 {shortId(item.trace_id)}</code>
      </summary>
      <div className="diagnostics-trace">
       <p className="helper">同一链路编号下的完整事件序列：<code>{item.trace_id}</code></p>
       {traces[item.trace_id]?.loading&&<p role="status">正在读取链路事件…</p>}
       {traces[item.trace_id]?.error&&<p role="alert" className="feedback error">{traces[item.trace_id].error}</p>}
       {!!traces[item.trace_id]?.items.length&&<ol>{traces[item.trace_id].items.map((event,index)=><li key={`${event.ts}-${index}`}>
        <span>{formatTime(event.ts)} · {event.event_type}{event.operation?` · ${event.operation}`:''}{event.result?` · ${event.result}`:''}{event.duration_ms!=null?` · ${event.duration_ms} ms`:''}</span>
        {event.reason&&<code className="diagnostics-reason">{event.reason}</code>}
        {event.message&&<code className="diagnostics-reason">{event.message}</code>}
       </li>)}</ol>}
       {traces[item.trace_id]&&!traces[item.trace_id].loading&&!traces[item.trace_id].error&&!traces[item.trace_id].items.length&&<p className="helper">该链路下没有可解析的日志事件。</p>}
      </div>
     </details>
    </li>)}</ul>
   </section>
  </>}
 </section>;
}
