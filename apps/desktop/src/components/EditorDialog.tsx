import React,{useEffect,useRef,useId} from 'react';
export function EditorDialog({open,title,onClose,children}:{open:boolean;title:string;onClose:()=>void;children:React.ReactNode}){
 const titleId=useId();
 const ref=useRef<HTMLDialogElement>(null),trigger=useRef<HTMLElement|null>(null),backdropPress=useRef(false);
 useEffect(()=>{const dialog=ref.current;if(!dialog)return;if(open&&!dialog.open){trigger.current=document.activeElement as HTMLElement;dialog.showModal();}else if(!open&&dialog.open){dialog.close();trigger.current?.focus();}},[open]);
 return <dialog ref={ref} className="editor-dialog" aria-labelledby={titleId} onPointerDown={e=>{const b=e.currentTarget.getBoundingClientRect();backdropPress.current=e.target===e.currentTarget&&(e.clientX<b.left||e.clientX>b.right||e.clientY<b.top||e.clientY>b.bottom);}} onClick={e=>{const b=e.currentTarget.getBoundingClientRect();const outside=e.target===e.currentTarget&&(e.clientX<b.left||e.clientX>b.right||e.clientY<b.top||e.clientY>b.bottom);if(backdropPress.current&&outside)onClose();backdropPress.current=false;}} onCancel={e=>{e.preventDefault();onClose();}}><div className="editor-dialog-head"><h2 id={titleId}>{title}</h2><button className="secondary" aria-label="关闭单品窗口" onClick={onClose}>关闭</button></div>{children}</dialog>;
}
