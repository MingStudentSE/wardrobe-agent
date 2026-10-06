#!/usr/bin/env python3
"""Run genuine local model download and segmentation from a relocated CLI/MCP.

Public licensed photo only. A disposable limited grant is passed over stdin;
reports never contain its token. Native developer loader paths are removed.
"""
import argparse,json,os,pathlib,subprocess,tempfile
p=argparse.ArgumentParser();p.add_argument('cli',type=pathlib.Path);p.add_argument('photo',type=pathlib.Path);p.add_argument('--report',type=pathlib.Path,required=True);args=p.parse_args()
cli=str(args.cli.resolve(strict=True));photo=str(args.photo.resolve(strict=True))
env={k:v for k,v in os.environ.items() if k not in ('LD_LIBRARY_PATH','DYLD_LIBRARY_PATH','DYLD_FALLBACK_LIBRARY_PATH','PKG_CONFIG_PATH','VCPKG_ROOT','VCPKGRS_DYNAMIC','WARDROBE_DATA_DIR','WARDROBE_CLIENT_TOKEN')}
env['PATH']=str(pathlib.Path(os.environ['SystemRoot'])/'System32') if os.name=='nt' else '/usr/bin:/bin'
with tempfile.TemporaryDirectory(prefix='wardrobe-cutout-packaged-') as temporary:
    database=str(pathlib.Path(temporary)/'wardrobe.sqlite');token=None
    def execute(operation,data,trusted=False):
        body=data if trusted else {'token':token,'input':data}
        command=[cli,'--db',database]
        command+=['--trusted-local',operation,'--input-stdin'] if trusted else [operation,'--client-stdin']
        process=subprocess.run(command,input=json.dumps(body),env=env,cwd=temporary,text=True,encoding='utf-8',capture_output=True,timeout=300)
        response=json.loads(process.stdout)
        if process.returncode or 'error' in response:raise RuntimeError(f'{operation}: '+json.dumps(response.get('error',{'code':'process_failed','message':process.stderr[:200]})))
        if token and token in process.stdout:raise RuntimeError('Client token leaked in business output')
        return response['result']
    member=execute('space.default',{},True)['member_id']
    operations=['assets.import','assets.get','images.model.status','images.model.download','images.cutout','images.worker.run']
    grant=execute('client.authorize',{'name':'Disposable cutout acceptance','mode':'limited','operations':operations,'member_ids':[member],'expires_at':None},True);token=grant['token'];del grant
    missing=execute('images.model.status',{'member_id':member})
    if missing['state']!='missing':raise RuntimeError('New isolated model cache must be missing')
    job=execute('images.model.download',{'member_id':member,'operation_key':'download'})
    downloaded=execute('images.worker.run',{'member_id':member,'job_id':job['id']})
    if downloaded['state']!='succeeded':raise RuntimeError('Actual official model download failed: '+json.dumps(downloaded['error']))
    model=execute('images.model.status',{'member_id':member})
    if not model['cached_offline']:raise RuntimeError('Downloaded model not verified')
    original=execute('assets.import',{'member_id':member,'path':photo})
    # Block HTTP connections through inherited proxies after acquisition; both
    # fresh CLI processes and MCP must now use the verified local cache.
    for key in ('HTTP_PROXY','HTTPS_PROXY','ALL_PROXY','http_proxy','https_proxy','all_proxy'):env[key]='http://127.0.0.1:9'
    env['NO_PROXY']='';env['no_proxy']=''
    cached=execute('images.model.download',{'member_id':member,'operation_key':'cached-offline-download'})
    cached=execute('images.worker.run',{'member_id':member,'job_id':cached['id']})
    if cached['state']!='succeeded':raise RuntimeError('Cached acquisition attempted network or failed')
    direct=execute('images.cutout',{'member_id':member,'asset_id':original['id'],'operation_key':'direct'})
    direct=execute('images.worker.run',{'member_id':member,'job_id':direct['id']})
    if direct['state']!='succeeded':raise RuntimeError('Actual local inference failed: '+json.dumps(direct['error']))
    if execute('assets.get',{'member_id':member,'id':original['id']})!=original:raise RuntimeError('Source asset mutated')
    process=subprocess.Popen([cli,'--db',database,'mcp'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env,cwd=temporary,text=True,encoding='utf-8')
    def rpc(method,params,identifier):
        process.stdin.write(json.dumps({'jsonrpc':'2.0','id':identifier,'method':method,'params':params})+'\n');process.stdin.flush();line=process.stdout.readline()
        if token in line:raise RuntimeError('Client token leaked in MCP output')
        response=json.loads(line)
        if 'error' in response:raise RuntimeError('MCP protocol error')
        return response['result']
    rpc('initialize',{'authorization_token':token},1)
    tools=rpc('tools/list',{},2)['tools']
    if {v['name'] for v in tools}!=set(operations+['capabilities']):raise RuntimeError('MCP grant discovery differs')
    def call(name,data,identifier):
        result=rpc('tools/call',{'name':name,'arguments':data},identifier);response=json.loads(result['content'][0]['text'])
        if 'error' in response:raise RuntimeError(name+': '+json.dumps(response['error']))
        return response
    mcp_job=call('images.cutout',{'member_id':member,'asset_id':original['id'],'operation_key':'mcp-offline'},3)
    mcp_done=call('images.worker.run',{'member_id':member,'job_id':mcp_job['id']},4)
    if mcp_done['state']!='succeeded' or mcp_done['result']['display']['id']!=direct['result']['display']['id']:raise RuntimeError('Fresh MCP inference differs from direct CLI')
    if call('assets.get',{'member_id':member,'id':original['id']},5)!=original:raise RuntimeError('MCP source retention differs')
    process.stdin.close();process.wait(timeout=30)
    if process.returncode:raise RuntimeError('MCP process failed')
    result={'result':'passed','cli':cli,'developer_loader_environment':'removed','model':{k:model[k] for k in ('model_id','bytes','sha256','license','source_url')},'official_model_acquisition':'downloaded and SHA256 verified','cached_restart':'passed with outbound HTTP proxy unavailable','direct_cli_inference':'passed','fresh_mcp_inference':'passed; identical display hash','source_asset':'unchanged','dimensions':[direct['result']['display']['width'],direct['result']['display']['height']],'cutout_display_hash':direct['result']['display']['id'],'gui_clean_install':'pending'}
    args.report.parent.mkdir(parents=True,exist_ok=True);args.report.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
