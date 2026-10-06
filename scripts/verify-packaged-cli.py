#!/usr/bin/env python3
"""Execute a relocated packaged CLI/MCP with developer loader variables removed.

The caller must extract/install the actual release bundle first. This verifies
capabilities, a real HEIC import, restart retrieval and stdio MCP equivalence;
it does not claim GUI or clean OS installation acceptance.
"""
import argparse
import json
import os
import pathlib
import subprocess
import tempfile

p=argparse.ArgumentParser();p.add_argument('cli',type=pathlib.Path);p.add_argument('heic',type=pathlib.Path);p.add_argument('--report',type=pathlib.Path,required=True);args=p.parse_args()
env={k:v for k,v in os.environ.items() if k not in ('LD_LIBRARY_PATH','DYLD_LIBRARY_PATH','DYLD_FALLBACK_LIBRARY_PATH','PKG_CONFIG_PATH','VCPKG_ROOT','VCPKGRS_DYNAMIC','WARDROBE_DATA_DIR')}
if os.name=='nt':env['PATH']=str(pathlib.Path(os.environ['SystemRoot'])/'System32')
else:env['PATH']='/usr/bin:/bin'
with tempfile.TemporaryDirectory(prefix='wardrobe-packaged-') as temporary:
    db=str(pathlib.Path(temporary)/'wardrobe.sqlite');cli=str(args.cli.resolve(strict=True))
    def execute(operation,data):
        completed=subprocess.run([cli,'--db',db,operation,json.dumps(data)],env=env,cwd=temporary,text=True,capture_output=True)
        if completed.returncode:
            failure={'cli':cli,'operation':operation,'exit_code':completed.returncode,'loader_error':completed.stderr[:2000],'developer_loader_environment':'removed','result':'failed'}
            try:
                application_response=json.loads(completed.stdout)
                if isinstance(application_response,dict) and 'error' in application_response:
                    failure['application_error']=application_response['error']
            except json.JSONDecodeError:
                pass
            args.report.parent.mkdir(parents=True,exist_ok=True);args.report.write_text(json.dumps(failure,indent=2)+'\n')
            raise RuntimeError(json.dumps(failure))
        response=json.loads(completed.stdout)
        if 'error' in response:raise RuntimeError(response['error'])
        return response['result']
    capabilities=execute('capabilities',{})
    member=execute('space.default',{})['member_id']
    imported=execute('assets.import',{'member_id':member,'path':str(args.heic.resolve(strict=True))})
    retrieved=execute('assets.get',{'member_id':member,'id':imported['id']})
    if retrieved!=imported:raise RuntimeError('Restart retrieval differs')
    request={'jsonrpc':'2.0','id':1,'method':'tools/call','params':{'name':'assets.get','arguments':{'member_id':member,'id':imported['id']}}}
    process=subprocess.run([cli,'--db',db,'mcp'],input=json.dumps(request)+'\n',env=env,cwd=temporary,text=True,capture_output=True,check=True)
    response=json.loads(process.stdout)
    result=json.loads(response['result']['content'][0]['text'])
    if result!=imported:raise RuntimeError('MCP retrieval differs')
    report={'cli':cli,'capabilities':len(capabilities['operations']),'heic_dimensions':[imported['display']['width'],imported['display']['height']],'restart':'passed','mcp':'passed','developer_loader_environment':'removed','gui_clean_install':'pending'}
    args.report.parent.mkdir(parents=True,exist_ok=True);args.report.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))
