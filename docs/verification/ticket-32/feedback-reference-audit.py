"""Actual public preview, valid transport/FKs, malformed feedback history references."""
import hashlib,json,pathlib,sqlite3,subprocess,sys,tempfile
CLI=str(pathlib.Path(sys.argv[1]).resolve())
def call(db,name,input):
 p=subprocess.run([CLI,'--db',str(db),'--trusted-local',name,'--input-stdin'],input=json.dumps(input),text=True,capture_output=True)
 response=json.loads(p.stdout)
 if p.returncode:return response
 result=response['result']
 if name.startswith('backup.') and 'job_id' in result:
  receipt=call(db,'maintenance.worker.run',{'id':result['job_id']})['result']
  return {'result':receipt['result']} if receipt['state']=='succeeded' else {'error':receipt['error']}
 return response
def okay(db,name,input):
 response=call(db,name,input);assert 'error' not in response,response;return response['result']
results={}
with tempfile.TemporaryDirectory(prefix='wardrobe32-feedback-refs-') as raw:
 root=pathlib.Path(raw);db=root/'source'/'wardrobe.sqlite'
 # The positive archive explicitly re-signs a revision3 parent to revision1 to model
 # a resolved immutable branch; current local API still increments from latest.
 # Committed licensed public shoe fixture; no user content.
 image=pathlib.Path(__file__).resolve().parents[3]/'crates/wardrobe-core/tests/fixtures/Zapato.jpg'
 def seed(member,key):
  stock=[okay(db,'clothing.create',{'member_id':member,'name':c,'category':c}) for c in ['top','bottom','shoes']]
  asset=okay(db,'assets.import',{'member_id':member,'path':str(image)})
  okay(db,'assets.adopt',{'member_id':member,'clothing_id':stock[0]['id'],'asset_id':asset['id']})
  intent=okay(db,'recommendation.generate',{'member_id':member,'operation_key':key,'timezone':'UTC'})
  first=okay(db,'feedback.record',{'member_id':member,'operation_key':key+'-feedback','intent_id':intent['id'],'proposal_index':0,'reaction':'like'})
  return first,asset
 first,asset=seed('default','refs-default')
 second=okay(db,'feedback.correct',{'member_id':'default','operation_key':'refs-correction','root_id':first['root_id'],'expected_revision':1,'scores':{'comfort':2}})
 third=okay(db,'feedback.revoke',{'member_id':'default','operation_key':'refs-revoke','root_id':first['root_id'],'expected_revision':2,'reason':'explicit nonlinear history fixture'})
 other=okay(db,'member.create',{'name':'temporary foreign member'})['member_id'];foreign,foreign_asset=seed(other,'refs-foreign')
 for case in ['initial_fake_predecessor','missing_predecessor','cross_member_predecessor','cross_member_asset_reference','valid_earlier_branch_parent']:
  package=root/case;okay(db,'backup.create',{'directory':str(package)})
  connection=sqlite3.connect(package/'facts.sqlite');connection.execute('PRAGMA foreign_keys=ON')
  if case=='cross_member_asset_reference':
   connection.execute('UPDATE feedback_asset_refs SET asset_id=?,member_id=? WHERE event_id=?',(foreign_asset['id'],other,first['id']))
  else:
   doc=dict(first if case=='initial_fake_predecessor' else third if case=='valid_earlier_branch_parent' else second)
   doc['supersedes_event_id']=first['id'] if case=='valid_earlier_branch_parent' else foreign['id'] if case=='cross_member_predecessor' else '00000000-0000-4000-8000-000000000099'
   connection.execute('UPDATE feedback_events SET document=? WHERE id=?',(json.dumps(doc),doc['id']))
   connection.execute("UPDATE feedback_receipts SET result=? WHERE json_extract(result,'$.id')=?",(json.dumps(doc),doc['id']))
  connection.commit();assert connection.execute('PRAGMA foreign_key_check').fetchone() is None;connection.close()
  manifest=json.loads((package/'manifest.json').read_text());data=(package/'facts.sqlite').read_bytes()
  for entry in manifest['files']:
   if entry['path']=='facts.sqlite':entry.update(bytes=len(data),sha256=hashlib.sha256(data).hexdigest())
  (package/'manifest.json').write_text(json.dumps(manifest))
  response=call(db,'backup.preview',{'directory':str(package)})
  results[case]={'rejected':response.get('error',{}).get('code')=='backup_invalid','code':response.get('error',{}).get('code'),'accepted':response.get('result',{}).get('valid',False)}
 print(json.dumps(results,indent=2),flush=True)
 assert all(result['rejected'] for case,result in results.items() if case!='valid_earlier_branch_parent'),'public preview accepted malformed feedback references'
 assert results['valid_earlier_branch_parent']['accepted'],'legal explicitly resigned earlier-parent history was rejected'
