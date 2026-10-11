import React,{useEffect,useRef,useState} from 'react';
import {api} from '../api';
import {errorMessage} from '../errorMessage';
import {settingsEntries} from './SettingsHub';
import {exportDiagnostics,formatBytes,isTauri,revealExportedFile,type ExportResult} from './diagnostics-export';
import {ExportResultNotice} from './diagnostics-export-result';
import {beginRecordingWindow,endRecordingWindow,injectedMaxSeconds,MAX_RECORDING_SECONDS} from './reproduction';
import './Diagnostics.css';
type RecentError={ts:string;level:string;event_type:string;run_id:string;trace_id?:string;operation?:string;result?:string;reason?:string;message?:string};
type Summary={
 database_schema_version:number;database_user_version:number;installation_generation:number;
 current_run_id:string;
 jobs_by_state:{queued:number;running:number;waiting_input:number;succeeded:number;failed:number;cancelled:number};
 provider_outcome_unknown:number;
 recent_errors:RecentError[];
 logs:{directory:string;total_bytes:number;file_count:number;retention_days:number;max_total_bytes:number;dropped_events:number};
};
type CheckItem={id:string;status:'pass'|'warn'|'fail';message:string;setting:string|null;detail:string};
type ChecksResult={schema_version:number;checks:CheckItem[];counts:{pass:number;warn:number;fail:number}};
type LogEvent={ts:string;level:string;event_type:string;run_id:string;operation?:string;duration_ms?:number;result?:string;reason?:string;message?:string};
type Scope='session'|'recent'|'all'|'window';
type TraceState={loading:boolean;items:LogEvent[];error?:string};
function formatTime(ts:string){const date=new Date(ts);return isNaN(date.getTime())?ts:date.toLocaleString('zh-CN',{hour12:false,month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',second:'2-digit'});}
function shortId(id:string){return id?id.slice(0,8):'—';}
const errorText=(e:unknown)=>errorMessage(e,'操作失败，请重试。');
function formatClock(ms:number){const total=Math.max(0,Math.floor(ms/1000));const minutes=Math.floor(total/60),seconds=total%60;return `${String(minutes).padStart(2,'0')}:${String(seconds).padStart(2,'0')}`;}
function formatLimit(seconds:number){const minutes=Math.floor(seconds/60);return minutes>0?`${minutes} 分钟`:`${seconds} 秒`;}
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
 const [openNote,setOpenNote]=useState(''),[openError,setOpenError]=useState('');
 // 日志目录取自 diagnostics.summary 的 logs.directory（日志引擎实际写入的目录，#73），
 // 不再由 data_directory.status 手拼路径。
 const logsDir=summary?.logs.directory??'';
 // 记录复现（#50）：开始/停止标记 + 以窗口为界的 kind=reproduction 导出；录制状态仅存
 // 内存（recordingRef 与组件状态），刷新页面即视为放弃，不做任何持久化。
 // 面板一经访问保持挂载（main.tsx 与其他面板同约定）：录制中切换视图/分区不丢状态，
 // 计时到点即使面板隐藏也照常自动停止并导出，回来即见提示。
 const [recording,setRecording]=useState<{startedAt:string;limitSeconds:number}|null>(null);
 const recordingRef=useRef<{startedAt:string;limitSeconds:number}|null>(null);
 const recBusyRef=useRef(false);
 const [recBusy,setRecBusy]=useState(false),[recError,setRecError]=useState(''),[recNote,setRecNote]=useState('');
 const [elapsed,setElapsed]=useState(0),[recResult,setRecResult]=useState<ExportResult|null>(null);
 async function refresh(){
  setLoading(true);setError('');setChecksLoading(true);setChecksError('');
  try{const result=await api<Summary>('diagnostics.summary');setSummary(result);}catch(e){setError(errorText(e));}finally{setLoading(false);}
  // 基础检查与健康摘要同一操作源（diagnostics.checks），独立加载互不阻塞
  try{const result=await api<ChecksResult>('diagnostics.checks');setChecks(result);}catch(e){setChecksError(errorText(e));}finally{setChecksLoading(false);}
 }
 useEffect(()=>{void refresh();},[]);
 // 记录计时（#50）：到点自动停止并提示。stopRecording 只读写 ref，闭包取首渲染版本安全。
 useEffect(()=>{
  const current=recording;
  if(!current)return;
  const deadline=Date.now()+current.limitSeconds*1000;
  setElapsed(0);
  const timer=setInterval(()=>{
   const remain=deadline-Date.now();
   if(remain<=0){clearInterval(timer);void stopRecording(true);}
   else setElapsed(current.limitSeconds*1000-remain);
  },250);
  return()=>clearInterval(timer);
 },[recording]);
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
   const result=await exportDiagnostics(scope==='window'?{from:new Date(from).toISOString(),to:new Date(to).toISOString()}:{scope});
   if(result)setExportResult(result);
  }catch(e){setExportError(errorText(e));}finally{setExporting(false);}
 }
 async function openLogsDirectory(){
  setOpenError('');setOpenNote('');
  if(!logsDir){setOpenError('暂时无法定位日志目录，请重试。');return;}
  if(isTauri()){
   try{await revealExportedFile(logsDir);setOpenNote('已在文件夹中显示日志目录。');}
   catch(e){setOpenError(errorText(e));}
  }else{
   setOpenNote(`当前是开发环境，无法打开文件夹窗口；日志目录：${logsDir}`);
  }
 }
 // 开始记录：写开始标记事件（返回的 started_at 即窗口起点），打开共享录制窗口，
 // 此后视图切换与对话框插桩经 reproduction.uiEvent 写入同一日志流。
 async function startRecording(){
  if(recBusyRef.current||recordingRef.current)return;
  recBusyRef.current=true;setRecBusy(true);setRecError('');setRecNote('');setRecResult(null);
  try{
   const result=await api<{started_at:string}>('diagnostics.reproduction.start',{});
   const limitSeconds=injectedMaxSeconds();
   beginRecordingWindow(limitSeconds);
   const state={startedAt:result.started_at,limitSeconds};
   recordingRef.current=state;
   setElapsed(0);
   setRecording(state);
  }catch(e){setRecError(errorText(e));}finally{recBusyRef.current=false;setRecBusy(false);}
 }
 // 停止记录：写停止标记事件（stopped_at 即窗口终点），随后以 [started_at, stopped_at]
 // 为界调用扩展后的 diagnostics.export（kind=reproduction + attach_checks）。
 // 自动停止（计时到点，auto=true）额外提示已达到最长记录时长。
 async function stopRecording(auto=false){
  const current=recordingRef.current;
  if(!current||recBusyRef.current)return;
  recBusyRef.current=true;setRecBusy(true);
  try{
   const stopped=await api<{stopped_at:string}>('diagnostics.reproduction.stop',{});
   endRecordingWindow();
   recordingRef.current=null;
   setRecording(null);
   const result=await exportDiagnostics({from:current.startedAt,to:stopped.stopped_at},{kind:'reproduction',attachChecks:true});
   if(auto)setRecNote(`已达到最长记录时长（${formatLimit(current.limitSeconds)}），已自动停止并导出复现记录。`);
   if(result)setRecResult(result);
  }catch(e){setRecError(errorText(e));endRecordingWindow();recordingRef.current=null;setRecording(null);}
  finally{recBusyRef.current=false;setRecBusy(false);}
 }
 const metrics:Metric[]=summary?[
  {label:'数据库模式',value:`v${summary.database_schema_version}`,note:summary.database_user_version===summary.database_schema_version?'与当前应用一致':`数据库为 v${summary.database_user_version}，需重启升级`},
  {label:'运行会话',value:shortId(summary.current_run_id),note:'本次启动的运行标识'},
  {label:'失败任务',value:String(summary.jobs_by_state.failed),note:`待输入 ${summary.jobs_by_state.waiting_input} · 已完成 ${summary.jobs_by_state.succeeded}`},
  {label:'结果未知',value:String(summary.provider_outcome_unknown),note:'已提交但未拿到供应商结果的任务'},
  {label:'日志占用',value:formatBytes(summary.logs.total_bytes),note:`${summary.logs.file_count} 个文件 · 保留 ${summary.logs.retention_days} 天${summary.logs.dropped_events?` · 丢弃 ${summary.logs.dropped_events} 条`:''}`},
 ]:[];
 const errors=[...(summary?.recent_errors??[])].reverse();
 // 最近错误按运行会话分组（复查修复）：本次启动一组在最前，更早会话按 run_id 各一组
 //（按该会话最近错误时间倒序）；组头显示会话标识与时间范围，条目内徽章不再重复。
 type ErrorGroup={run_id:string;current:boolean;items:RecentError[]};
 const errorGroups:ErrorGroup[]=(()=>{
  const byRun=new Map<string,RecentError[]>();
  for(const item of errors){
   const list=byRun.get(item.run_id)??[];
   if(!list.length)byRun.set(item.run_id,list);
   list.push(item);
  }
  const groups=[...byRun.entries()].map(([run_id,items])=>({run_id,items,current:run_id===summary?.current_run_id}));
  groups.sort((a,b)=>(a.current?1:0)-(b.current?1:0)||String(b.items[0]?.ts).localeCompare(String(a.items[0]?.ts)));
  return groups;
 })();
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
   <section className="diagnostics-section diagnostics-reproduction" aria-label="记录复现">
    <h3>记录复现</h3>
    <p className="diagnostics-reproduction-help">点「开始记录复现」后正常使用应用复现问题，再回到这里点「停止并导出」，得到只含这段时间的单文件复现记录（含操作时间线与基础检查）。记录最长 {formatLimit(MAX_RECORDING_SECONDS)}，到点自动停止；刷新或关闭页面即放弃本次记录。</p>
    {recording?<div className="diagnostics-recording" role="status">
     <span className="diagnostics-recording-dot" aria-hidden="true"></span>
     <span className="diagnostics-recording-text">正在记录操作 · 已记录 <strong className="diagnostics-recording-timer">{formatClock(elapsed)}</strong> / 最长 {formatLimit(recording.limitSeconds)}</span>
     <button disabled={recBusy} onClick={()=>void stopRecording(false)}>{recBusy?'正在停止并导出…':'停止并导出'}</button>
    </div>:<div className="diagnostics-recording-idle">
     <button disabled={recBusy} onClick={()=>void startRecording()}>{recBusy?'正在开始…':'开始记录复现'}</button>
    </div>}
    {recNote&&<p role="status" className="diagnostics-reproduction-note">{recNote}</p>}
    {recError&&<p role="alert" className="feedback error">{recError}</p>}
    {recResult&&<ExportResultNotice result={recResult} heading="复现记录已导出"/>}
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
    {exportResult&&<ExportResultNotice result={exportResult}/>}
   </section>
   <section className="diagnostics-section" aria-label="最近错误">
    <h3>最近错误</h3>
    {!errors.length&&<div className="settings-empty"><h3>暂无错误记录</h3><p>操作失败时会出现在这里，并附上排查用的链路编号。</p></div>}
    <ul className="diagnostics-error-groups">
     {errorGroups.map(group=><li key={group.run_id} className="diagnostics-error-group">
      <div className="diagnostics-error-group-head">
       <span className={group.current?'diagnostics-session-badge current':'diagnostics-session-badge'}>{group.current?'本次启动':'更早会话'}</span>
       <code className="diagnostics-error-group-run">会话 {shortId(group.run_id)}</code>
       <span className="diagnostics-error-group-range">{formatTime(group.items[group.items.length-1].ts)} – {formatTime(group.items[0].ts)} · {group.items.length} 条</span>
      </div>
      <ul className="diagnostics-errors">{group.items.map(item=>item.trace_id&&<li key={`${item.ts}-${item.trace_id}`}>
       <details className="diagnostics-error" onToggle={event=>{const target=event.currentTarget;if(target.open&&!traces[item.trace_id!])void loadTrace(item.trace_id!);}}>
        <summary>
         <span className="diagnostics-error-time">{formatTime(item.ts)}</span>
         <span className="diagnostics-error-operation">{item.operation??item.event_type}</span>
         <code className="diagnostics-error-code">{item.result??'error'}</code>
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
     </li>)}
    </ul>
   </section>
  </>}
 </section>;
}
