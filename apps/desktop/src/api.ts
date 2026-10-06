import { invoke } from '@tauri-apps/api/core';
export async function api<T>(operation:string,input:object={}):Promise<T>{
 if (import.meta.env.DEV && !('__TAURI_INTERNALS__' in window)) { const response=await fetch('/__wardrobe_dev_api',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({operation,input})}); const body=await response.json();if(body.error)throw body.error;return body.result; }
 return invoke('application',{operation,input});
}
