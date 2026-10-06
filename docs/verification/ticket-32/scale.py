"""Full 10k/100k synthetic facts, seeded from public API shapes, and >256 MiB original payload."""
import copy,json,pathlib,shutil,sqlite3,struct,subprocess,sys,tempfile,time,uuid,zlib
CLI=str(pathlib.Path(sys.argv[1]).resolve())
def command(db,name,args):
 started=time.monotonic()
 p=subprocess.Popen([CLI,'--db',str(db),'--trusted-local',name,'--input-stdin'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
 pending=json.dumps(args)
 while True:
  try:
   output,diagnostics=p.communicate(pending,timeout=5);break
  except subprocess.TimeoutExpired:
   pending=None
   installation=pathlib.Path(db).parent.parent
   print(json.dumps({'phase':name,'still_running':True,'free_bytes':shutil.disk_usage(installation).free,'sqlite_and_wal_bytes':sum(f.stat().st_size for f in installation.rglob('*') if f.is_file() and ('sqlite' in f.name or f.name.endswith('-wal')))}),flush=True)
 response=json.loads(output)
 if diagnostics:print(json.dumps({'operation':name,'diagnostic_lines':diagnostics.strip().splitlines()}),flush=True)
 if p.returncode:raise RuntimeError({"operation":name,"response":response,"diagnostic_stderr":diagnostics})
 result=response['result']
 if name in ('backup.create','backup.preview','backup.restore','data_directory.move'):
  assert result['state']=='queued' and result['job_id'],result
  print(json.dumps({'operation':name,'durable_queued_reply_ms':(time.monotonic()-started)*1000,'job_id':result['job_id']}),flush=True)
  completed=command(db,'maintenance.worker.run',{'id':result['job_id']})
  if completed['state']!='succeeded':raise RuntimeError({'operation':name,'job':completed})
  return completed['result']
 return result
def chunk(kind,payload):return struct.pack('>I',len(payload))+kind+payload+struct.pack('>I',zlib.crc32(kind+payload)&0xffffffff)
def encoded(value):return json.dumps(value,separators=(',',':'))
with tempfile.TemporaryDirectory(prefix='wardrobe32-scale-') as raw:
 root=pathlib.Path(raw);db=root/'source'/'wardrobe.sqlite';target=root/'target'/'wardrobe.sqlite';package=root/'backup'
 command(db,'space.default',{});assets=[]
 for n in range(9):
  png=root/f'original-{n}.png'
  png.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',16,16,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\0'+bytes([n+50,90,120])*16)*16))+chunk(b'paDd',bytes([n])*30000000)+chunk(b'IEND',b''))
  assets.append(command(db,'assets.import',{'member_id':'default','path':str(png)}));png.unlink()
 garment=command(db,'clothing.create',{'member_id':'default','name':'capacity seed','category':'top'})
 command(db,'assets.adopt',{'member_id':'default','clothing_id':garment['id'],'asset_id':assets[0]['id']})
 garment=command(db,'clothing.get',{'member_id':'default','id':garment['id']})
 companions=[command(db,'clothing.create',{'member_id':'default','name':category,'category':category}) for category in ['bottom','shoes']]
 intent=command(db,'recommendation.generate',{'member_id':'default','operation_key':'scale-feedback-intent','timezone':'UTC','fixed_ids':[garment['id']]})
 feedback=command(db,'feedback.record',{'member_id':'default','operation_key':'scale-feedback','intent_id':intent['id'],'proposal_index':0,'reaction':'like'})
 command(db,'feedback.correct',{'member_id':'default','operation_key':'scale-feedback-correct','root_id':feedback['root_id'],'expected_revision':1,'reaction':'dislike','scores':{'comfort':2}})
 command(db,'feedback.revoke',{'member_id':'default','operation_key':'scale-feedback-revoke','root_id':feedback['root_id'],'expected_revision':2,'reason':'capacity observation fixture'})
 feedback_history=command(db,'feedback.history',{'member_id':'default','root_id':feedback['root_id']})
 event=command(db,'wear.record',{'member_id':'default','clothing_ids':[garment['id']],'local_date':'2020-01-01','timezone':'UTC','operation_key':'scale-seed'})
 member=garment['member_id'];ids=[garment['id']]+[c['id'] for c in companions]+[str(uuid.uuid4()) for _ in range(9997)]
 connection=sqlite3.connect(db);connection.execute('PRAGMA foreign_keys=ON')
 with connection:
  for n,id in enumerate(ids[3:],3):
   document=copy.deepcopy(garment);document.update(id=id,name=f'capacity garment {n}')
   connection.execute('INSERT INTO clothing VALUES(?,?,?)',(id,member,encoded(document)))
  request=json.loads(connection.execute("SELECT request FROM wear_receipts WHERE operation_key='scale-seed'").fetchone()[0])
  for n in range(1,100000):
   id=str(uuid.uuid4());document=copy.deepcopy(event);document.update(id=id,root_event_id=id)
   item=document['items'][0];item['clothing_id']=ids[n%10000];item['snapshot']['id']=ids[n%10000]
   text=encoded(document)
   connection.execute('INSERT INTO wear_events VALUES(?,?,?,?,?)',(id,member,id,1,text))
   row_request=copy.deepcopy(request);row_request['input']['operation_key']=f'scale-{n}';row_request['input']['clothing_ids']=[ids[n%10000]]
   connection.execute('INSERT INTO wear_receipts VALUES(?,?,?,?)',(member,f'scale-{n}',encoded(row_request),text))
   connection.execute('INSERT INTO wear_asset_refs VALUES(?,?,?)',(id,assets[0]['id'],member))
   connection.execute('INSERT INTO audit(entity_id,operation,document) VALUES(?,?,?)',(id,'wear.record',text))
   connection.execute('INSERT INTO outbox(entity_id,document) VALUES(?,?)',(id,text))
 connection.close();elapsed={}
 print(json.dumps({"phase":"seeded","database_bytes":db.stat().st_size,"media_bytes":sum(p.stat().st_size for p in (db.parent/"assets").rglob("*") if p.is_file())}),flush=True)
 for name,args in [('backup.create',{'directory':str(package),'include_originals':True}),('backup.preview',{'directory':str(package)})]:
  start=time.monotonic();result=command(db,name,args);elapsed[name]=time.monotonic()-start;print(json.dumps({'phase':name,'seconds':elapsed[name]}),flush=True)
  if name=='backup.preview':assert result['incoming']['clothing']==10000 and result['incoming']['wear_events']==100000
 manifest=json.loads((package/'manifest.json').read_text());payload=sum(item['bytes'] for item in manifest['files']);assert payload>256*1024*1024
 shutil.rmtree(db.parent)
 start=time.monotonic();command(target,'backup.restore',{'directory':str(package),'confirm_overwrite':True});elapsed['backup.restore']=time.monotonic()-start;print(json.dumps({'phase':'backup.restore','seconds':elapsed['backup.restore']}),flush=True)
 preview=command(target,'backup.preview',{'directory':str(package)});assert preview['current']['clothing']==10000 and preview['current']['wear_events']==100000
 assert command(target,'clothing.get',{'member_id':member,'id':ids[-1]})['id']==ids[-1]
 assert command(target,'wear.get',{'member_id':member,'id':event['id']})==event
 assert command(target,'feedback.history',{'member_id':member,'root_id':feedback['root_id']})==feedback_history
 for asset in assets:
  read=command(target,'assets.get',{'member_id':member,'id':asset['id']});original=pathlib.Path(read['original']['local_path']);assert original.exists() and original.stat().st_size>30000000
 print(json.dumps({'clothing':10000,'wear_events':100000,'originals':9,'feedback_revisions_and_observation_restored':True,'package_bytes':payload,'source_removed_before_restore':True,'restart_public_reads':True,'fixture_seed':'public API shapes expanded by explicit synthetic SQL transaction','seconds':elapsed},indent=2))
