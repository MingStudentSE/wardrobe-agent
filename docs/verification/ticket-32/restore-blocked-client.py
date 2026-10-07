"""Actual restore and an already authenticated MCP writer; temporary synthetic installs only."""
import json,pathlib,select,struct,subprocess,sys,tempfile,time,zlib
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
def chunk(kind,payload):return struct.pack('>I',len(payload))+kind+payload+struct.pack('>I',zlib.crc32(kind+payload)&0xffffffff)
with tempfile.TemporaryDirectory(prefix='wardrobe32-restore-writer-') as raw:
 root=pathlib.Path(raw);source=root/'incoming'/'wardrobe.sqlite';target=root/'target'/'wardrobe.sqlite'
 command(source,'space.default',{});command(target,'space.default',{})
 package=root/'backup';command(source,'backup.create',{'directory':str(package)})
 for n in range(4):
  png=root/f'original-{n}.png'
  png.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',16,16,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\0'+bytes([n+50,90,120])*16)*16))+chunk(b'paDd',bytes([n])*35000000)+chunk(b'IEND',b''))
  command(target,'assets.import',{'member_id':'default','path':str(png)});png.unlink()
 grant=command(target,'client.authorize',{'name':'temporary blocked restore verifier','mode':'full','operations':[],'member_ids':None,'expires_at':None})
 mcp=subprocess.Popen([CLI,'--db',str(target),'mcp'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
 def send(id,method,params):mcp.stdin.write(json.dumps({'jsonrpc':'2.0','id':id,'method':method,'params':params})+'\n');mcp.stdin.flush()
 send(1,'initialize',{'authorization_token':grant['token']});assert json.loads(mcp.stdout.readline())['id']==1
 submitted=subprocess.run([CLI,'--db',str(target),'--trusted-local','backup.restore','--input-stdin'],input=json.dumps({'directory':str(package),'confirm_overwrite':True}),text=True,capture_output=True)
 job=json.loads(submitted.stdout)['result'];assert job['state']=='queued'
 restore=subprocess.Popen([CLI,'--db',str(target),'--trusted-local','maintenance.worker.run','--input-stdin'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
 restore.stdin.write(json.dumps({'id':job['job_id']}));restore.stdin.close()
 deadline=time.monotonic()+20
 while not list(target.parent.glob('backup-rollback/*.staging-*/media/*')):
  if restore.poll() is not None:raise RuntimeError('restore ended before prebackup observer')
  if time.monotonic()>deadline:raise RuntimeError('prebackup copy not observed')
 send(2,'tools/call',{'name':'clothing.create','arguments':{'member_id':'default','name':'stale grant cannot write into restored facts','category':'top'}})
 blocked=not select.select([mcp.stdout],[],[],0.03)[0];assert blocked,'writer was not actually observed blocked'
 restore.wait(timeout=30);receipt=json.loads(restore.stdout.read());assert restore.returncode==0,receipt
 response=json.loads(mcp.stdout.readline())
 denied=bool(response.get('error') or response.get('result',{}).get('isError'))
 mcp.stdin.close();mcp.wait(timeout=10)
 assert denied, 'previously authenticated blocked writer was allowed after restore removed its grant'
 assert not command(target,'clothing.list',{'member_id':'default'})['items']
 print(json.dumps({'observed_blocked':blocked,'stale_authorized_writer_denied':denied,'restored_facts_unchanged':True},indent=2))
