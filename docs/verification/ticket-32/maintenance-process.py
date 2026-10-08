"""Actual process kills at explicitly gated debug publication seams, plus natural running cancellation."""
import json,os,pathlib,struct,subprocess,sys,tempfile,time,zlib
CLI=str(pathlib.Path(sys.argv[1]).resolve())
def call(db,op,args):
 p=subprocess.run([CLI,'--db',str(db),'--trusted-local',op,'--input-stdin'],input=json.dumps(args),text=True,capture_output=True,timeout=60)
 response=json.loads(p.stdout)
 if p.returncode:raise RuntimeError(response)
 return response['result']
def maintained(db,op,args):
 job=call(db,op,args);done=call(db,'maintenance.worker.run',{'id':job['job_id']});assert done['state']=='succeeded',done;return done['result']
def worker(db,id,gate=None):
 env=os.environ.copy()
 if gate is not None:gate.mkdir();env['WARDROBE_BACKUP_TEST_GATE']=str(gate)
 p=subprocess.Popen([CLI,'--db',str(db),'--trusted-local','maintenance.worker.run','--input-stdin'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env)
 p.stdin.write(json.dumps({'id':id}));p.stdin.close();return p
def observed(process,predicate):
 deadline=time.monotonic()+10
 while time.monotonic()<deadline:
  value=predicate()
  if value:return value
  if process.poll() is not None:raise RuntimeError({'worker_finished_before_observer':process.returncode,'stdout':process.stdout.read(),'stderr':process.stderr.read()})
  time.sleep(.001)
 raise RuntimeError('phase was not observed')
def kill(process):process.kill();process.wait(timeout=10);assert process.returncode==-9
result={}
with tempfile.TemporaryDirectory(prefix='wardrobe32-published-create-') as raw:
 root=pathlib.Path(raw);db=root/'source/wardrobe.sqlite';directory=root/'package';gate=root/'gate'
 fact=call(db,'clothing.create',{'member_id':'default','name':'owned package fact','category':'top'})
 queued=call(db,'backup.create',{'directory':str(directory)});p=worker(db,queued['job_id'],gate);observed(p,lambda:(gate/'create-published').exists());assert directory.exists()
 kill(p);recovered=call(db,'maintenance.worker.run',{'id':queued['job_id']});assert recovered['state']=='succeeded',recovered
 assert maintained(db,'backup.preview',{'directory':str(directory)})['incoming']['clothing']==1
 result['create_rename_before_receipt_commit']={'actual_exit':p.returncode,'explicit_debug_gate':True,'same_job_recovered':True,'one_package':len(list(root.glob('package*')))==1}
with tempfile.TemporaryDirectory(prefix='wardrobe32-committed-restore-') as raw:
 root=pathlib.Path(raw);source=root/'incoming/wardrobe.sqlite';target=root/'target/wardrobe.sqlite';directory=root/'package';gate=root/'gate'
 incoming=call(source,'clothing.create',{'member_id':'default','name':'committed incoming','category':'top'});maintained(source,'backup.create',{'directory':str(directory)})
 old=call(target,'clothing.create',{'member_id':'default','name':'old fact','category':'bottom'})
 queued=call(target,'backup.restore',{'directory':str(directory),'confirm_overwrite':True});p=worker(target,queued['job_id'],gate);observed(p,lambda:(gate/'restore-committed').exists());kill(p)
 receipt=call(target,'maintenance.get',{'id':queued['job_id']});assert receipt['state']=='succeeded' and receipt['result']['restored'],receipt
 assert call(target,'clothing.get',{'member_id':'default','id':incoming['id']})==incoming
 rollback=receipt['result']['rollback_backup'];assert maintained(target,'backup.preview',{'directory':rollback})['incoming']['clothing']==1
 result['restore_commit_before_worker_finishes']={'actual_exit':p.returncode,'explicit_debug_gate':True,'atomic_receipt_survives':True,'restart_reads_incoming':True,'actual_prebackup_valid':True}
with tempfile.TemporaryDirectory(prefix='wardrobe32-published-directory-') as raw:
 root=pathlib.Path(raw);db=root/'source/wardrobe.sqlite';directory=root/'moved';gate=root/'gate'
 fact=call(db,'clothing.create',{'member_id':'default','name':'same local installation','category':'top'})
 queued=call(db,'data_directory.move',{'directory':str(directory)});p=worker(db,queued['job_id'],gate);observed(p,lambda:(gate/'directory-published').exists());kill(p)
 receipt=call(db,'maintenance.get',{'id':queued['job_id']});assert receipt['state']=='succeeded',receipt
 assert call(db,'clothing.get',{'member_id':'default','id':fact['id']})==call(directory/'wardrobe.sqlite','clothing.get',{'member_id':'default','id':fact['id']})==fact
 result['directory_active_before_worker_finishes']={'actual_exit':p.returncode,'explicit_debug_gate':True,'receipt_reconciled':True,'old_and_new_public_paths_agree':True}
def chunk(kind,payload):return struct.pack('>I',len(payload))+kind+payload+struct.pack('>I',zlib.crc32(kind+payload)&0xffffffff)
with tempfile.TemporaryDirectory(prefix='wardrobe32-running-cancel-') as raw:
 root=pathlib.Path(raw);db=root/'source/wardrobe.sqlite';directory=root/'cancelled-package'
 for n in range(4):
  picture=root/f'fixture-{n}.png';picture.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',16,16,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\0'+bytes([80+n,90,100])*16)*16))+chunk(b'paDd',bytes([n])*35000000)+chunk(b'IEND',b''));call(db,'assets.import',{'member_id':'default','path':str(picture)});picture.unlink()
 queued=call(db,'backup.create',{'directory':str(directory),'include_originals':True});p=worker(db,queued['job_id'])
 observed(p,lambda:any(path.stat().st_size>1000000 for path in root.glob('cancelled-package.staging-*/media/*') if path.is_file()))
 cancelled=call(db,'maintenance.cancel',{'id':queued['job_id']});assert cancelled['state']=='cancelled',cancelled
 p.wait(timeout=30);done=json.loads(p.stdout.read())['result'];assert done['state']=='cancelled' and not directory.exists(),done
 assert call(db,'maintenance.get',{'id':queued['job_id']})['state']=='cancelled'
 result['natural_running_copy_cancellation']={'phase_observed':'media file >1MB','worker_exit':p.returncode,'cancelled_not_resurrected':True,'no_package_published':True}
print(json.dumps(result,ensure_ascii=False,indent=2),flush=True)
