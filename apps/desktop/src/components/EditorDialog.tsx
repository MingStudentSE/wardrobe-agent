import React,{useEffect,useRef} from 'react';
export function EditorDialog({open,title,onClose,children}:{open:boolean;title:string;onClose:()=>void;children:React.ReactNode}){
 const ref=useRef<HTMLDialogElement>(null),trigger=useRef<HTMLElement|null>(null);
 useEffect(()=>{const dialog=ref.current;if(!dialog)return;if(open&&!dialog.open){trigger.current=document.activeElement as HTMLElement;dialog.showModal();}else if(!open&&dialog.open){dialog.close();trigger.current?.focus();}},[open]);
 return <dialog ref={ref} className="editor-dialog" aria-labelledby="editor-dialog-title" onCancel={e=>{e.preventDefault();onClose();}}><div className="editor-dialog-head"><h2 id="editor-dialog-title">{title}</h2><button className="secondary" aria-label="关闭单品窗口" onClick={onClose}>关闭</button></div>{children}</dialog>;
}
