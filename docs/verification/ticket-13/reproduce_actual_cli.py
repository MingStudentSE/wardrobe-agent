import json,subprocess,tempfile,time,pathlib,os,re
root=pathlib.Path(__file__).resolve().parents[3];endpoint=os.environ['WARDROBE_RECOGNITION_REAL_ENDPOINT'];assert re.fullmatch(r'http://127\.0\.0\.1:\d+/v1',endpoint);db=pathlib.Path(tempfile.mkdtemp(prefix='wardrobe13-actual-'))/'db';cli=root/'target/debug/wardrobe-cli'
def run(op,args={}):
 p=subprocess.run([str(cli),'--db',str(db),'--trusted-local',op,'--input-stdin'],input=json.dumps(args),text=True,capture_output=True);value=json.loads(p.stdout)
 if p.returncode:raise RuntimeError(value)
 return value['result']
p=run('provider.save',{'name':'Actual SmolVLM','endpoint':endpoint,'model':'wardrobe-smolvlm','capabilities':['vision'],'authentication':'none'})
results=[]
for fn,label in [('OKM_Shirt_Front.jpg','public shirt'),('Zapato.jpg','public shoe'),('Jeans.jpg','public jeans')]:
 c=run('clothing.create',{'member_id':'default','name':label,'category':'other'});a=run('assets.import',{'member_id':'default','path':str(root/'crates/wardrobe-core/tests/fixtures'/fn)});run('assets.adopt',{'member_id':'default','clothing_id':c['id'],'asset_id':a['id']})
 j=run('recognition.create',{'member_id':'default','operation_key':fn,'clothing_id':c['id'],'provider_id':p['id'],'asset_ids':[a['id']],'upload_role':'product_display','confirm_upload':True});start=time.monotonic();done=run('recognition.process',{'member_id':'default','id':j['id']});elapsed=time.monotonic()-start
 result={'fixture':fn,'job':done,'elapsed_seconds':elapsed,'unchanged_clothing':run('clothing.get',{'member_id':'default','id':c['id']})}
 if done['state']=='succeeded':result['proposal']=run('recognition.get',{'member_id':'default','id':done['result']['suggestion_id']})
 results.append(result);print(fn,done['state'],result.get('proposal',{}).get('raw_text'),elapsed,flush=True)
(root/'docs/verification/ticket-13/actual-cli.json').write_text(json.dumps({'database':str(db),'results':results},ensure_ascii=False,indent=2))
