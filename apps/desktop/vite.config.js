import { defineConfig } from 'vite';
import { execFile } from 'node:child_process';
import { fileURLToPath } from 'node:url';
// Development-only loopback bridge to the REAL CLI/Application API, never a mock.
// This middleware is absent from the production bundle and Tauri runtime.
export default defineConfig({server:{host:'127.0.0.1'},plugins:[{name:'wardrobe-development-api',configureServer(server){server.middlewares.use('/__wardrobe_dev_api',(req,res)=>{
 if(req.method!=='POST'){res.statusCode=405;res.end();return;}
 let body='';req.on('data',chunk=>{body+=chunk;if(body.length>65536)req.destroy();});req.on('end',()=>{
 let request;try{request=JSON.parse(body);}catch{res.statusCode=400;res.end(JSON.stringify({error:{code:'invalid_json',message:'输入不是有效 JSON'}}));return;}
 const cli=fileURLToPath(new URL('../../target/debug/wardrobe-cli',import.meta.url));
 execFile(cli,[String(request.operation),JSON.stringify(request.input??{})],{env:process.env,maxBuffer:1024*1024},(error,stdout)=>{res.setHeader('Content-Type','application/json');if(stdout){res.end(stdout);}else{res.statusCode=503;res.end(JSON.stringify({error:{code:'dev_bridge_unavailable',message:'暂时无法读取衣橱，请重试。'}}));}});
 });
 });}}]});
