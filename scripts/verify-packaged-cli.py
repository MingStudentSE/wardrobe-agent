#!/usr/bin/env python3
"""Verify a relocated native CLI/MCP, authenticated by a disposable local grant.

Remove developer loader variables; never persist or print the raw client token.
This does not claim GUI, signing, or clean OS installation acceptance.
"""
import argparse
import json
import os
import pathlib
import subprocess
import tempfile

p=argparse.ArgumentParser();p.add_argument('cli',type=pathlib.Path);p.add_argument('heic',type=pathlib.Path);p.add_argument('--report',type=pathlib.Path,required=True);args=p.parse_args()
env={k:v for k,v in os.environ.items() if k not in ('LD_LIBRARY_PATH','DYLD_LIBRARY_PATH','DYLD_FALLBACK_LIBRARY_PATH','PKG_CONFIG_PATH','VCPKG_ROOT','VCPKGRS_DYNAMIC','WARDROBE_DATA_DIR','WARDROBE_CLIENT_TOKEN')}
if os.name=='nt':env['PATH']=str(pathlib.Path(os.environ['SystemRoot'])/'System32')
else:env['PATH']='/usr/bin:/bin'
with tempfile.TemporaryDirectory(prefix='wardrobe-packaged-') as temporary:
    db=str(pathlib.Path(temporary)/'wardrobe.sqlite');cli=str(args.cli.resolve(strict=True));token=None
    def fail(operation,completed=None,message=None):
        failure={'cli':cli,'operation':operation,'developer_loader_environment':'removed','result':'failed'}
        if completed is not None:
            failure.update(exit_code=completed.returncode,loader_error=completed.stderr[:2000])
            try:
                response=json.loads(completed.stdout)
                if isinstance(response,dict) and 'error' in response:failure['application_error']=response['error']
            except json.JSONDecodeError:pass
        if message is not None:failure['message']=message
        # Guard the diagnostic path as well: no caller-supplied or echoed token.
        encoded=json.dumps(failure,indent=2)
        if token:encoded=encoded.replace(token,'[redacted]')
        args.report.parent.mkdir(parents=True,exist_ok=True);args.report.write_text(encoded+'\n');raise RuntimeError(encoded)
    def execute(operation,data,trusted=False):
        command=[cli,'--db',db]
        if trusted:command+=['--trusted-local',operation,'--input-stdin'];body=data
        else:command +=[operation,'--client-stdin'];body={'token':token,'input':data}
        completed=subprocess.run(command,input=json.dumps(body),env=env,cwd=temporary,text=True,encoding="utf-8",capture_output=True)
        if completed.returncode:fail(operation,completed)
        if token and token in completed.stdout:fail(operation,message='Client token appeared in business output')
        try:response=json.loads(completed.stdout)
        except json.JSONDecodeError:fail(operation,message='Invalid application JSON')
        if 'error' in response:fail(operation,completed)
        return response['result']
    member=execute('space.default',{},trusted=True)['member_id']
    grant=execute('client.authorize',{'name':'Disposable packaged verification','mode':'limited','operations':['assets.import','assets.get'],'member_ids':[member],'expires_at':None},trusted=True)
    token=grant['token'];del grant
    capabilities=execute('capabilities',{})
    imported=execute('assets.import',{'member_id':member,'path':str(args.heic.resolve(strict=True))})
    retrieved=execute('assets.get',{'member_id':member,'id':imported['id']})
    if retrieved!=imported:fail('assets.get',message='Restart retrieval differs')
    unauthenticated=subprocess.run([cli,'--db',db,'assets.get','--input-stdin'],input=json.dumps({'member_id':member,'id':imported['id']}),env=env,cwd=temporary,text=True,encoding="utf-8",capture_output=True)
    try:denied=json.loads(unauthenticated.stdout)
    except json.JSONDecodeError:fail('unauthenticated assets.get',unauthenticated)
    if denied.get('error',{}).get('code')!='unauthorized':fail('unauthenticated assets.get',message='Unregistered client was not denied')
    requests=[{'jsonrpc':'2.0','id':1,'method':'initialize','params':{'authorization_token':token}}, {'jsonrpc':'2.0','id':2,'method':'tools/list','params':{}}, {'jsonrpc':'2.0','id':3,'method':'tools/call','params':{'name':'assets.get','arguments':{'member_id':member,'id':imported['id']}}}]
    process=subprocess.run([cli,'--db',db,'mcp'],input=''.join(json.dumps(v)+'\n' for v in requests),env=env,cwd=temporary,text=True,encoding="utf-8",capture_output=True)
    if process.returncode:fail('mcp',process)
    if token in process.stdout:fail('mcp',message='Client token appeared in MCP output')
    responses=[json.loads(line) for line in process.stdout.splitlines()]
    if len(responses)!=3 or 'error' in responses[0]:fail('mcp initialize',message='Authenticated initialization failed')
    tools=responses[1].get('result',{}).get('tools',[])
    if {v['name'] for v in tools}!={'capabilities','assets.import','assets.get'}:fail('mcp tools/list',message='Scope discovery differs')
    result=json.loads(responses[2]['result']['content'][0]['text'])
    if result!=imported:fail('mcp assets.get',message='MCP retrieval differs')
    probe={'jsonrpc':'2.0','id':1,'method':'tools/call','params':{'name':'assets.get','arguments':{'member_id':member,'id':imported['id']}}}
    process=subprocess.run([cli,'--db',db,'mcp'],input=json.dumps(probe)+'\n',env=env,cwd=temporary,text=True,encoding="utf-8",capture_output=True)
    if process.returncode:fail('unauthenticated mcp',process)
    response=json.loads(process.stdout);error=json.loads(response['result']['content'][0]['text'])
    if error.get('error',{}).get('code')!='unauthorized':fail('unauthenticated mcp',message='Unregistered MCP client was not denied')
    report={'cli':cli,'capabilities':len(capabilities['operations']),'authorization':'limited operation and member grant; stdin token','unauthenticated_cli':'denied','unauthenticated_mcp':'denied','heic_dimensions':[imported['display']['width'],imported['display']['height']],'restart':'passed','mcp':'passed','developer_loader_environment':'removed','gui_clean_install':'pending'}
    args.report.parent.mkdir(parents=True,exist_ok=True);args.report.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))
