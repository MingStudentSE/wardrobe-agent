import {api} from '../api';

// 记录复现的共享录制状态（#50）：诊断页拥有开始/停止/计时，插桩点（main.tsx 视图切换、
// 对话框组件提交等）只关心「当前是否在记录」。状态仅存进程内存——刷新页面即视为放弃，
// 不做任何持久化；记录不改变任何日志级别与脱敏行为（ui_event 只写白名单事件）。
export type UiEventType='view_switch'|'dialog_open'|'dialog_submit';
// 产品上限：记录最长 10 分钟，到点由诊断页计时自动停止并提示。
export const MAX_RECORDING_SECONDS=600;
// 测试接缝：Playwright 经 localStorage 注入更短上限驱动自动停止（仅接受 (0,600] 的整数，
// 产品路径永远拿到默认 600）。
export function injectedMaxSeconds():number{
 try{
  const raw=Number(localStorage.getItem('wardrobe-reproduction-max-seconds'));
  if(Number.isFinite(raw)&&raw>0&&raw<=MAX_RECORDING_SECONDS)return Math.floor(raw);
 }catch{/* 无 localStorage 环境按默认 */}
 return MAX_RECORDING_SECONDS;
}
let recordingUntil=0;
export function beginRecordingWindow(maxSeconds:number){recordingUntil=Date.now()+maxSeconds*1000;}
export function endRecordingWindow(){recordingUntil=0;}
export function recordingActive(){return recordingUntil>0&&Date.now()<recordingUntil;}
// 关键交互插桩的唯一入口：仅记录期间写入；尽力而为，绝不阻塞或改变交互本身的结果。
// label 是前端固定界面文案（如「穿搭库」「记录穿着」），不是用户输入内容。
export async function uiEvent(type:UiEventType,label:string){
 if(!recordingActive())return;
 try{await api('diagnostics.ui_event',{type,label});}catch{/* 日志尽力而为 */}
}
