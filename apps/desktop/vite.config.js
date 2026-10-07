import { defineConfig } from 'vite';
import { execFile } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import {mkdtemp,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
// Development-only loopback bridge to the REAL CLI/Application API, never a mock.
// This middleware is absent from the production bundle and Tauri runtime.
export default defineConfig({server:{host:'127.0.0.1'},plugins:[{name:'wardrobe-development-api',configureServer(server){server.middlewares.use('/__wardrobe_dev_upload',async(req,res)=>{
 if(req.method!=='POST'||(req.headers.origin&&req.headers.origin!==`http://${req.headers.host}`)){res.statusCode=403;res.end();return;}
 const ext={'image/jpeg':'jpg','image/png':'png','image/webp':'webp'}[req.headers['content-type']];
 res.setHeader('Content-Type','application/json');let directory;
 try{if(!ext)throw new Error('请选择 JPG、PNG 或 WebP 图片。');const chunks=[];let size=0;for await(const chunk of req){size+=chunk.length;if(size>20*1024*1024)throw new Error('请选择 20 MB 以内的照片。');chunks.push(chunk);}directory=await mkdtemp(join(tmpdir(),'wardrobe-upload-'));const path=join(directory,'photo.'+ext);await writeFile(path,Buffer.concat(chunks));const member=new URL(req.url,'http://localhost').searchParams.get('member');const cli=fileURLToPath(new URL('../../target/debug/wardrobe-cli',import.meta.url));const stdout=await new Promise((resolve,reject)=>{const child=execFile(cli,['--trusted-local','assets.import','--input-stdin'],{env:process.env,maxBuffer:32*1024*1024},(error,out)=>out?resolve(out):reject(error));child.stdin.end(JSON.stringify({member_id:member,path}));});res.end(stdout);}catch(error){res.statusCode=400;res.end(JSON.stringify({error:{message:error.message||'图片导入失败，请重试。'}}));}finally{if(directory)await rm(directory,{recursive:true,force:true});}
 });server.middlewares.use('/__wardrobe_dev_api',(req,res)=>{
 if(req.headers.origin&&req.headers.origin!==`http://${req.headers.host}`){res.statusCode=403;res.end();return;}
 if(req.method!=='POST'){res.statusCode=405;res.end();return;}
 let body='';req.on('data',chunk=>{body+=chunk;if(body.length>65536)req.destroy();});req.on('end',()=>{
 let request;try{request=JSON.parse(body);}catch{res.statusCode=400;res.end(JSON.stringify({error:{code:'invalid_json',message:'输入不是有效 JSON'}}));return;}
 const cli=fileURLToPath(new URL('../../target/debug/wardrobe-cli',import.meta.url));
 const child=execFile(cli,['--trusted-local',String(request.operation),'--input-stdin'],{env:process.env,maxBuffer:32*1024*1024},(error,stdout)=>{res.setHeader('Content-Type','application/json');if(stdout){res.end(stdout);}else{res.statusCode=503;res.end(JSON.stringify({error:{code:'dev_bridge_unavailable',message:'暂时无法读取衣橱，请重试。'}}));}});
 child.stdin.end(JSON.stringify(request.input??{}));
 });
 });}}]});
