"""Real local TCP POST/SIGKILL/same-space older-backup boundary; no account/key/user data."""
import hashlib,http.server,json,pathlib,sqlite3,subprocess,sys,tempfile,threading,time,uuid
CLI=str(pathlib.Path(sys.argv[1]).resolve())
response_mode='hold'
count=0;seen=threading.Condition();release=threading.Event()
class Provider(http.server.BaseHTTPRequestHandler):
 def do_POST(self):
  global count
  request=json.loads(self.rfile.read(int(self.headers.get('Content-Length',0))))
  mode=response_mode
  with seen:count+=1;seen.notify_all()
  if mode=='hold':release.wait(20)
  try:
   if mode=='valid':
    prompt=json.loads(request['messages'][1]['content']);items=prompt['candidate_outfits'][0]['items']
    body=json.dumps({'choices':[{'message':{'content':json.dumps({'proposals':[{'items':items,'explanation':'local protocol candidate'}]})}}]}).encode()
    self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
   else:self.send_response(503);self.end_headers()
  except (BrokenPipeError,ConnectionResetError):pass
 def log_message(self,*args):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Provider);server.daemon_threads=True
threading.Thread(target=server.serve_forever,daemon=True).start()
def call(db,op,args):
 p=subprocess.run([CLI,'--db',str(db),'--trusted-local',op,'--input-stdin'],input=json.dumps(args),text=True,capture_output=True,timeout=60)
 result=json.loads(p.stdout)
 if p.returncode:raise RuntimeError(result)
 return result['result']
def maintained(db,op,args):
 receipt=call(db,op,args);job=call(db,'maintenance.worker.run',{'id':receipt['job_id']})
 assert job['state']=='succeeded',job
 return job['result']
def worker(db,id,member="default"):
 p=subprocess.Popen([CLI,'--db',str(db),'--trusted-local','recommendation.worker.run','--input-stdin'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
 p.stdin.write(json.dumps({'member_id':member,'id':id}));p.stdin.close();return p
def wait_count(expected,seconds):
 with seen:return seen.wait_for(lambda:count>=expected,timeout=seconds)
try:
 with tempfile.TemporaryDirectory(prefix='wardrobe32-submission-restore-') as raw:
  root=pathlib.Path(raw);db=root/'source'/'wardrobe.sqlite';package=root/'backup'
  for category in ['top','bottom','shoes']:call(db,'clothing.create',{'member_id':'default','name':category,'category':category})
  provider=call(db,'provider.save',{'name':'temporary TCP protocol','endpoint':f'http://127.0.0.1:{server.server_port}/v1','model':'fixture-text','capabilities':['text'],'authentication':'none'})
  maintained(db,'backup.create',{'directory':str(package)})
  intent={'member_id':'default','operation_key':'unknown-before-restore','timezone':'UTC','strategy':'ai','provider_id':provider['id']}
  first=call(db,'recommendation.generate',intent);process=worker(db,first['job_id'])
  assert wait_count(1,5),'first actual POST not received'
  process.kill();process.wait(timeout=10)
  before=call(db,'jobs.get',{'member_id':'default','id':first['job_id']});assert before['submitted_at'],before
  restore_result=maintained(db,'backup.restore',{'directory':str(package),'confirm_overwrite':True})
  retry=subprocess.run([CLI,'--db',str(db),'--trusted-local','recommendation.generate','--input-stdin'],input=json.dumps(intent),text=True,capture_output=True)
  restored=json.loads(retry.stdout)
  if retry.returncode:
   assert restored['error']['code']=='submission_already_sent',restored
   duplicate=wait_count(2,0.3)
  else:
   second=worker(db,restored['result']['job_id']);duplicate=wait_count(2,3)
   if second.poll() is None:second.kill()
   second.wait(timeout=10)
  print(json.dumps({'first_worker_exit':process.returncode,'first_job':before,'after_restore_receipt':restored,'provider_posts':count,'duplicate_post_observed':duplicate},ensure_ascii=False),flush=True)
  assert count==1,'same-space restore allowed a second POST for an already submitted unknown operation intent'
  maintained(db,'backup.restore',{'directory':restore_result['rollback_backup'],'confirm_overwrite':True})
  rollback_retry=subprocess.run([CLI,'--db',str(db),'--trusted-local','recommendation.generate','--input-stdin'],input=json.dumps(intent),text=True,capture_output=True)
  assert json.loads(rollback_retry.stdout)['error']['code']=='submission_already_sent' and count==1
  print(json.dumps({'rollback_restore_keeps_submission_guard':True,'provider_posts':count}),flush=True)
 # Actual member isolation, queued and completed outcomes, plus portable omission.
 response_mode='valid'
 with tempfile.TemporaryDirectory(prefix='wardrobe32-submission-matrix-') as raw:
  root=pathlib.Path(raw);db=root/'source'/'wardrobe.sqlite';package=root/'older'
  for category in ['top','bottom','shoes']:call(db,'clothing.create',{'member_id':'default','name':category,'category':category})
  provider=call(db,'provider.save',{'name':'temporary matrix protocol','endpoint':f'http://127.0.0.1:{server.server_port}/v1','model':'fixture-text','capabilities':['text'],'authentication':'none'})
  maintained(db,'backup.create',{'directory':str(package)})
  intent={'member_id':'default','operation_key':'queued-then-completed','timezone':'UTC','strategy':'ai','provider_id':provider['id']}
  queued=call(db,'recommendation.generate',intent);baseline=count
  maintained(db,'backup.restore',{'directory':str(package),'confirm_overwrite':True})
  resumed=call(db,'recommendation.generate',intent);completed=call(db,'recommendation.worker.run',{'member_id':'default','id':resumed['job_id']});assert completed['state']=='succeeded',completed
  assert count==baseline+1,'never-submitted queued intent must be explicitly rebuildable'
  fact=call(db,'recommendation.get',{'member_id':'default','id':completed['result']['intent_id']})
  newer=root/'newer';maintained(db,'backup.create',{'directory':str(newer)})
  maintained(db,'backup.restore',{'directory':str(package),'confirm_overwrite':True})
  denied=subprocess.run([CLI,'--db',str(db),'--trusted-local','recommendation.generate','--input-stdin'],input=json.dumps(intent),text=True,capture_output=True)
  assert json.loads(denied.stdout)['error']['code']=='submission_already_sent' and count==baseline+1
  maintained(db,'backup.restore',{'directory':str(newer),'confirm_overwrite':True})
  cached=call(db,'recommendation.generate',intent);assert cached['intent_id']==fact['id'] and 'job_id' not in cached
  assert call(db,'recommendation.get',{'member_id':'default','id':fact['id']})==fact and count==baseline+1
  other=call(db,'member.create',{'name':'different temporary member'})['member_id']
  for category in ['top','bottom','shoes']:call(db,'clothing.create',{'member_id':other,'name':category,'category':category})
  another=dict(intent,member_id=other);receipt=call(db,'recommendation.generate',another);done=call(db,'recommendation.worker.run',{'member_id':other,'id':receipt['job_id']});assert done['state']=='succeeded' and count==baseline+2
  portable=root/'portable';maintained(db,'backup.create',{'directory':str(portable)})
  snapshot=sqlite3.connect(portable/'facts.sqlite');names={row[0] for row in snapshot.execute("SELECT name FROM sqlite_master WHERE type='table'")};snapshot.close();assert 'local_submission_guards' not in names and 'maintenance_jobs' not in names
  local=sqlite3.connect(db);hashes=[row[0].encode() for row in local.execute('SELECT payload_hash FROM local_submission_guards')];local.close();bytes_=(portable/'facts.sqlite').read_bytes();assert all(hash_ not in bytes_ for hash_ in hashes)
  # Explicit synthetic identity collision: same member UUID in a different space.
  changed=root/'distinct-space';maintained(db,'backup.create',{'directory':str(changed)})
  snapshot=sqlite3.connect(changed/'facts.sqlite');snapshot.execute('UPDATE workspace SET id=?',(str(uuid.uuid4()),));snapshot.execute('DELETE FROM recommendation_intents');snapshot.commit();snapshot.close()
  manifest=json.loads((changed/'manifest.json').read_text());bytes_=(changed/'facts.sqlite').read_bytes()
  for entry in manifest['files']:
   if entry['path']=='facts.sqlite':entry.update(bytes=len(bytes_),sha256=hashlib.sha256(bytes_).hexdigest())
  (changed/'manifest.json').write_text(json.dumps(manifest));maintained(db,'backup.restore',{'directory':str(changed),'confirm_overwrite':True})
  # Synthetic distinct-space backup deliberately has the same member UUID and
  # no recommendation intent; public submission must not inherit the old space guard.
  alternate=dict(intent);receipt=call(db,'recommendation.generate',alternate);done=call(db,'recommendation.worker.run',{'member_id':'default','id':receipt['job_id']});assert done['state']=='succeeded' and count==baseline+3
  print(json.dumps({'queued_never_submitted_rebuilt':True,'completed_old_backup_rejected':True,'completed_fact_backup_readable_without_fake_job':True,'other_member_same_key_submitted':True,'ledger_absent_from_portable_tables_and_bytes':True,'distinct_space_same_member_uuid_same_key_submitted':True,'matrix_provider_posts':count-baseline}),flush=True)
 response_mode='hold'
 with tempfile.TemporaryDirectory(prefix='wardrobe32-cancel-submission-') as raw:
  root=pathlib.Path(raw);db=root/'source'/'wardrobe.sqlite';package=root/'backup'
  for category in ['top','bottom','shoes']:call(db,'clothing.create',{'member_id':'default','name':category,'category':category})
  provider=call(db,'provider.save',{'name':'cancelled TCP fixture','endpoint':f'http://127.0.0.1:{server.server_port}/v1','model':'fixture-text','capabilities':['text'],'authentication':'none'})
  maintained(db,'backup.create',{'directory':str(package)});intent={'member_id':'default','operation_key':'cancelled-submission','timezone':'UTC','strategy':'ai','provider_id':provider['id']}
  baseline=count;queued=call(db,'recommendation.generate',intent);process=worker(db,queued['job_id']);assert wait_count(baseline+1,5)
  cancelled=call(db,'jobs.cancel',{'member_id':'default','id':queued['job_id']});assert cancelled['state']=='cancelled'
  process.kill();process.wait(timeout=10);maintained(db,'backup.restore',{'directory':str(package),'confirm_overwrite':True})
  denied=subprocess.run([CLI,'--db',str(db),'--trusted-local','recommendation.generate','--input-stdin'],input=json.dumps(intent),text=True,capture_output=True)
  assert json.loads(denied.stdout)['error']['code']=='submission_already_sent' and not wait_count(baseline+2,.3)
  print(json.dumps({'cancelled_submitted_intent_still_blocked_after_restore':True,'provider_posts':count-baseline}),flush=True)
finally:
 release.set();server.shutdown();server.server_close()
