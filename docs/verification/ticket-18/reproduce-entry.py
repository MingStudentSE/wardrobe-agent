"""Actual three-entry parity: temporary persisted SQLite, limited grant via stdin only."""
import json,subprocess,sys,tempfile,os
cli=sys.argv[1]
with tempfile.TemporaryDirectory(prefix='wardrobe-learning-') as folder:
 db=os.path.join(folder,'db')
 def trusted(op,input):
  response=json.loads(subprocess.check_output([cli,'--trusted-local','--db',db,op,'--input-stdin'],input=json.dumps(input).encode()));assert 'error' not in response,response;return response['result']
 member=trusted('space.default',{})['member_id']
 clothes=[trusted('clothing.create',{'member_id':member,'name':name,'category':cat,'color':[color]}) for name,cat,color in [('Red','top','red'),('Blue','top','blue'),('裤','bottom','black'),('鞋','shoes','black')]]
 wear=trusted('wear.record',{'member_id':member,'operation_key':'actual','clothing_ids':[clothes[1]['id']],'local_date':'2020-01-01','timezone':'UTC'})
 ops=['learning.'+v for v in ['get','set','edit','delete','reset','recompute','history']]+['learning.worker.run','jobs.get','jobs.cancel','recommendation.generate','wear.list']
 token=trusted('client.authorize',{'name':'temporary learning verification','mode':'limited','operations':ops,'member_ids':[member],'expires_at':None})['token']
 def call(op,input):
  response=json.loads(subprocess.check_output([cli,'--db',db,op,'--client-stdin'],input=json.dumps({'token':token,'input':{'member_id':member,**input}}).encode()));assert 'error' not in response,response;return response['result']
 def mcp(op,input):
  request={'jsonrpc':'2.0','id':1,'method':'tools/call','params':{'authorization_token':token,'name':op,'arguments':{'member_id':member,**input}}}
  response=json.loads(subprocess.check_output([cli,'--db',db,'mcp'],input=(json.dumps(request)+'\n').encode()));assert 'error' not in response and not response['result'].get('isError'),response;return json.loads(response['result']['content'][0]['text'])
 receipts=[]
 def compute(transport,input):
  receipt=transport('learning.recompute',input)
  assert receipt['job_id'] and receipt['state']=='queued',receipt
  durable=call('jobs.get',{'id':receipt['job_id']});assert durable['state']=='queued'
  finished=mcp('learning.worker.run',{'job_id':receipt['job_id']});assert finished['state']=='succeeded',finished
  assert call('jobs.get',{'id':receipt['job_id']})==finished
  assert mcp('learning.worker.run',{'job_id':receipt['job_id']})==finished
  receipts.append({'receipt':receipt,'reopened_job':durable,'terminal':finished})
  return mcp('learning.get',{})
 p=compute(mcp,{'operation_key':'learn'});assert any(v['value']=='blue' and v['weight']==1 for v in p['inferred'])
 request={'operation_key':'explicit','dimension':'color','value':'blue','weight':5,'note':'explicit choice'}
 explicit=call('learning.set',request);assert mcp('learning.set',request)==explicit
 recommendation=mcp('recommendation.generate',{'operation_key':'preferred','timezone':'UTC','limit':1});assert any(v['clothing_id']==clothes[1]['id'] for v in recommendation['proposals'][0]['items'])
 edited=mcp('learning.edit',{'operation_key':'edit','root_id':explicit['root_id'],'expected_revision':1,'weight':-2});assert edited['revision']==2
 deleted=call('learning.delete',{'operation_key':'forget','dimension':'color','value':'blue'})
 after=compute(mcp,{'operation_key':'afterforget'});assert not any(v['value']=='blue' for v in after['inferred'])
 reset=mcp('learning.reset',{'operation_key':'reset','scope':'all'})
 afterreset=compute(call,{'operation_key':'afterreset'});assert afterreset['inferred']==[]
 rebuilt=compute(call,{'operation_key':'rebuild','include_prior':True});assert any(v['value']=='blue' for v in rebuilt['inferred'])
 history=[];cursor=None
 while True:
  page=mcp('learning.history',{'limit':2,**({'cursor':cursor} if cursor else {})});history+=page['items'];cursor=page['next_cursor']
  if not cursor:break
 assert len(history)==8
 assert len(call('wear.list',{})['items'])==1
 print(json.dumps({'durable_async_receipts':receipts,'limited_operations':ops,'explicit':explicit,'edited':edited,'deleted':deleted,'reset':reset,'profile':p,'rebuilt':rebuilt,'recommendation':recommendation,'history_count':len(history),'wear_preserved':wear,'effective_wear_count':1,'credential_recorded':False,'temporary_data_removed_on_exit':True},ensure_ascii=False,indent=2))
