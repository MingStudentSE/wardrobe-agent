import React from 'react';
import './journal-settings.css';
export const settingsEntries=[
 {id:'preferences',group:'穿衣习惯',title:'穿衣偏好',description:'告诉衣橱你喜欢什么，让推荐更贴近你。',icon:'heart'},
 {id:'model',group:'智能与连接',title:'模型与连接',description:'管理用于推荐、识别和图片生成的模型。',icon:'spark'},
 {id:'tasks',group:'智能与连接',title:'任务中心',description:'查看识别、生成与导入的处理进度。',icon:'layers'},
 {id:'backup',group:'数据与设备',title:'备份与恢复',description:'为衣橱留一份备份，管理本机数据位置。',icon:'archive'},
 {id:'encryption',group:'数据与设备',title:'加密与设备',description:'保护衣橱资料，管理配对设备和恢复材料。',icon:'lock'},
 {id:'clients',group:'高级连接',title:'CLI / MCP 与外部 Agent',description:'连接其他工具，查看和管理客户端授权。',icon:'code'},
];
function SettingsIcon({kind}:{kind:string}){const paths:Record<string,React.ReactNode>={heart:<path d="M20 5a5 5 0 0 0-8 2 5 5 0 0 0-8-2c-4 4 2 10 8 15 6-5 12-11 8-15Z"/>,spark:<path d="m12 3 3 6 6 3-6 3-3 6-3-6-6-3 6-3Z"/>,layers:<path d="m12 3 9 5-9 5-9-5 9-5Zm-9 10 9 5 9-5M3 18l9 5 9-5"/>,archive:<><rect x="3" y="4" width="18" height="5" rx="1"/><path d="M5 9v11h14V9M9 13h6"/></>,lock:<><rect x="5" y="10" width="14" height="11" rx="2"/><path d="M8 10V7a4 4 0 0 1 8 0v3M12 14v3"/></>,code:<path d="m8 7-5 5 5 5m8-10 5 5-5 5m-3-12-2 14"/>};return <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">{paths[kind]}</svg>;}
export function SettingsHub({active,onSelect}:{active:string;onSelect:(id:string)=>void}){
 if(active!=='overview'){const entry=settingsEntries.find(e=>e.id===active);return <div className="settings-breadcrumb"><button className="secondary" onClick={()=>onSelect('overview')}>← 返回设置</button><span>{entry?.group} / {entry?.title}</span></div>;}
 return <section className="settings-hub" aria-label="设置目录">{['穿衣习惯','智能与连接','数据与设备','高级连接'].map(group=><section className="settings-group" key={group}><h2>{group}</h2><div>{settingsEntries.filter(e=>e.group===group).map(entry=><button className="settings-card secondary" key={entry.id} onClick={()=>onSelect(entry.id)}><span className="settings-icon"><SettingsIcon kind={entry.icon}/></span><span><strong>{entry.title}</strong><small>{entry.description}</small></span><span className="settings-arrow" aria-hidden="true">↗</span></button>)}</div></section>)}</section>;
}
