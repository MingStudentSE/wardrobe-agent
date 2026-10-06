"""Actual CLI / persistent MCP processes; only temporary synthetic installations."""
import json,os,pathlib,select,signal,subprocess,sys,tempfile,time
CLI=str(pathlib.Path(sys.argv[1]).resolve())
def command(db,name,args):
 p=subprocess.run([CLI,'--db',str(db),'--trusted-local',name,'--input-stdin'],input=json.dumps(args),text=True,capture_output=True)
 response=json.loads(p.stdout)
 if p.returncode:raise RuntimeError(response)
 result=response['result']
 if name in ('backup.create','backup.preview','backup.restore','data_directory.move'):
  assert result['state']=='queued' and result['job_id'],result
  completed=command(db,'maintenance.worker.run',{'id':result['job_id']})
  if completed['state']!='succeeded':raise RuntimeError({'operation':name,'job':completed})
  return completed['result']
 return result
def start_move(db,target):return subprocess.Popen([CLI,'--db',str(db),'--trusted-local','maintenance.worker.run','--input-stdin'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
def launch(db,target):
 submitted=subprocess.run([CLI,'--db',str(db),'--trusted-local','data_directory.move','--input-stdin'],input=json.dumps({'directory':str(target)}),text=True,capture_output=True)
 job=json.loads(submitted.stdout)['result'];assert job['state']=='queued' and not target.exists()
 p=start_move(db,target);p._wardrobe_job_id=job['job_id'];p.stdin.write(json.dumps({'id':job['job_id']}));p.stdin.close();return p
def copying(root,process):
 deadline=time.monotonic()+15
 while time.monotonic()<deadline:
  candidates=list(root.glob('moved.staging-*/assets/process-fixture.bin'))
  if candidates:return candidates[0]
  if process.poll() is not None:raise RuntimeError('move finished before copy observer')
 raise RuntimeError('copy not observed')
def fixture(root):
 old=root/'source';old.mkdir();db=old/'wardrobe.sqlite'
 fact=command(db,'clothing.create',{'member_id':'default','name':'源事实','category':'top'})
 assets=old/'assets';assets.mkdir()
 with (assets/'process-fixture.bin').open('wb') as f:
  chunk=b'owned local migration fixture\x00'*2048
  for _ in range(2300):f.write(chunk)
 return old,db,fact
result={}
with tempfile.TemporaryDirectory(prefix='wardrobe32-blocked-') as raw:
 root=pathlib.Path(raw);old,db,fact=fixture(root);target=root/'moved'
 grant=command(db,'client.authorize',{'name':'temporary process verifier','mode':'full','operations':[],'member_ids':None,'expires_at':None})
 mcp=subprocess.Popen([CLI,'--db',str(db),'mcp'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
 def send(id,method,params):mcp.stdin.write(json.dumps({'jsonrpc':'2.0','id':id,'method':method,'params':params})+'\n');mcp.stdin.flush()
 send(1,'initialize',{'authorization_token':grant['token']});assert json.loads(mcp.stdout.readline())['id']==1
 move=launch(db,target);copying(root,move)
 assert not (old/'.wardrobe-location.json').exists(),'writer must begin before publication'
 send(2,'tools/call',{'name':'clothing.create','arguments':{'member_id':'default','name':'发布前已打开连接的写入','category':'shoes'}})
 blocked=not select.select([mcp.stdout],[],[],0.03)[0]
 assert blocked,'public writer was not observed blocked'
 move.wait(timeout=20);receipt=json.loads(move.stdout.read());assert move.returncode==0,receipt
 response=json.loads(mcp.stdout.readline())
 if response.get('error') or response.get('result',{}).get('isError'):
  send(3,'tools/call',{'name':'clothing.create','arguments':{'member_id':'default','name':'发布前已打开连接的写入','category':'shoes'}});response=json.loads(mcp.stdout.readline())
 assert not response.get('error') and not response['result'].get('isError'),response
 created=json.loads(response['result']['content'][0]['text'])
 assert command(target/'wardrobe.sqlite','clothing.get',{'member_id':'default','id':created['id']})==created
 mcp.stdin.close();mcp.wait(timeout=10)
 result['blocked_existing_mcp_writer']={'observed_blocked':blocked,'new_directory_contains_acknowledged_write':True}
with tempfile.TemporaryDirectory(prefix='wardrobe32-kill-') as raw:
 root=pathlib.Path(raw);old,db,fact=fixture(root);target=root/'moved';move=launch(db,target);copying(root,move);move.kill();move.wait(timeout=10)
 assert command(db,'clothing.get',{'member_id':'default','id':fact['id']})==fact
 assert not (old/'.wardrobe-location.json').exists()
 retry=command(db,'maintenance.retry',{'id':move._wardrobe_job_id});assert retry['state']=='queued'
 recovered=command(db,'maintenance.worker.run',{'id':move._wardrobe_job_id});assert recovered['state']=='succeeded',recovered
 assert command(db,'clothing.get',{'member_id':'default','id':fact['id']})==fact
 result['actual_process_kill_during_copy']={'exit':move.returncode,'old_source_readable':True,'location_not_switched_before_explicit_recovery':True,'explicit_recovery_succeeded':True}
if os.name=='posix' and os.geteuid()!=0:
 with tempfile.TemporaryDirectory(prefix='wardrobe32-publish-failure-') as raw:
  root=pathlib.Path(raw);old,db,fact=fixture(root);target=root/'moved';move=launch(db,target);copying(root,move);old.chmod(0o500)
  try:
   move.wait(timeout=20);failure=json.loads(move.stdout.read());assert move.returncode==0 and failure['result']['state']=='failed',failure
  finally:old.chmod(0o700)
  assert command(db,'clothing.get',{'member_id':'default','id':fact['id']})==fact
  assert not (old/'.wardrobe-location.json').exists()
  result['actual_marker_publish_permission_failure']={'error':failure['result']['error']['code'],'old_source_readable':True,'location_not_switched':True}
print(json.dumps(result,ensure_ascii=False,indent=2))
