import {defineConfig} from '@playwright/test';
import {mkdtempSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';

// Web 端 e2e：生产构建 UI + 真实 wardrobe-web 服务 + 真实 SQLite（ADR0026 交付验证）。
// 需要：npm run build 产出 dist；cargo build -p wardrobe-web（或 WARDROBE_WEB_BIN 指向现成二进制）。
// DYLD_LIBRARY_PATH 等原生媒体环境继承自外层 shell（见 docs/agents/build-storage.md）。
const port=process.env.WARDROBE_WEB_TEST_PORT??'5188';
const dataDir=process.env.WARDROBE_WEB_TEST_DATA??mkdtempSync(join(tmpdir(),'wardrobe-web-e2e-'));
process.env.WARDROBE_WEB_TEST_DATA=dataDir;
const serviceBin=process.env.WARDROBE_WEB_BIN??fileURLToPath(new URL('../../../target/debug/wardrobe-web',import.meta.url));
const distDir=fileURLToPath(new URL('./dist',import.meta.url));
const authFile=fileURLToPath(new URL('./tests-web/.auth/wardrobe.json',import.meta.url));

export default defineConfig({
 testDir:'./tests-web',workers:1,fullyParallel:false,
 use:{baseURL:`http://127.0.0.1:${port}`,channel:'chrome',locale:'zh-CN'},
 projects:[
  {name:'bootstrap',testMatch:/00-bootstrap\.spec\.ts/,use:{storageState:{cookies:[],origins:[]}}},
  {name:'app',dependencies:['bootstrap'],testIgnore:/00-bootstrap\.spec\.ts/,use:{storageState:authFile}},
 ],
 webServer:{
  command:`"${serviceBin}" --port ${port} --data-dir "${dataDir}" --web-dist "${distDir}"`,
  url:`http://127.0.0.1:${port}/health`,
  reuseExistingServer:false,
 },
});
