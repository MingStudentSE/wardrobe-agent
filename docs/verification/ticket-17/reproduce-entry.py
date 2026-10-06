"""Actual CLI / stdin MCP against temporary SQLite and limited temporary grant.
No token is printed, persisted in evidence, or put on the command line.
"""
import json,subprocess,sys,tempfile,os
cli=sys.argv[1]
with tempfile.TemporaryDirectory(prefix='wardrobe-feedback-') as folder:
 db=os.path.join(folder,'db')
 def trusted(op,input):
  result=json.loads(subprocess.check_output([cli,'--trusted-local','--db',db,op,'--input-stdin'],input=json.dumps(input).encode()))
  assert 'error' not in result,result
  return result['result']
 member=trusted('space.default',{})['member_id']
 clothes=[trusted('clothing.create',{'member_id':member,'name':name,'category':cat}) for name,cat in [('A','top'),('B','top'),('C','top'),('裤','bottom'),('鞋','shoes')]]
 ops=['recommendation.generate','recommendation.get','recommendation.change','feedback.record','feedback.get','feedback.list','feedback.correct','feedback.revoke','feedback.history','feedback.try_another','wear.record','wear.list']
 grant=trusted('client.authorize',{'name':'temporary feedback acceptance','mode':'limited','operations':ops,'member_ids':[member],'expires_at':None})
 token=grant['token']
 def cli_call(op,input):
  response=json.loads(subprocess.check_output([cli,'--db',db,op,'--client-stdin'],input=json.dumps({'token':token,'input':input}).encode()))
  assert 'error' not in response,response
  return response['result']
 def mcp(op,input):
  request={'jsonrpc':'2.0','id':1,'method':'tools/call','params':{'authorization_token':token,'name':op,'arguments':input}}
  response=json.loads(subprocess.check_output([cli,'--db',db,'mcp'],input=(json.dumps(request)+'\n').encode()))
  assert 'error' not in response and not response['result'].get('isError'),response
  return json.loads(response['result']['content'][0]['text'])
 r=cli_call('recommendation.generate',{'member_id':member,'operation_key':'generate','timezone':'UTC','fixed_ids':[clothes[-1]['id']],'limit':1})
 f_input={'member_id':member,'operation_key':'like','intent_id':r['id'],'proposal_index':0,'reaction':'like','scores':{'overall':5,'comfort':None,'style':4}}
 f=mcp('feedback.record',f_input);assert cli_call('feedback.record',f_input)==f
 assert cli_call('feedback.get',{'member_id':member,'id':f['id']})==f
 assert cli_call('wear.list',{'member_id':member})['items']==[]
 next=mcp('feedback.try_another',{'member_id':member,'operation_key':'another','intent_id':r['id'],'proposal_index':0});assert next['recommendation']['proposals'][0]['items']!=r['proposals'][0]['items']
 corrected=cli_call('feedback.correct',{'member_id':member,'root_id':f['root_id'],'expected_revision':1,'operation_key':'correct','scores':{'comfort':2}})
 revoked=mcp('feedback.revoke',{'member_id':member,'root_id':f['root_id'],'expected_revision':2,'operation_key':'revoke','reason':'correction'})
 assert len(mcp('feedback.history',{'member_id':member,'root_id':f['root_id']})['items'])==3
 actual_input={'member_id':member,'intent_id':r['id'],'proposal_index':0,'reaction':'actual_wore','operation_key':'actual','wear':{'confirmed':True,'local_date':'2020-01-01','timezone':'Pacific/Honolulu'}}
 actual=cli_call('feedback.record',actual_input);assert mcp('feedback.record',actual_input)==actual
 changed_input={'member_id':member,'intent_id':r['id'],'proposal_index':0,'reaction':'changed','operation_key':'changed','wear':{'confirmed':True,'local_date':'2020-01-02','timezone':'UTC','clothing_ids':[clothes[1]['id'],clothes[3]['id'],clothes[4]['id']]}}
 changed=mcp('feedback.record',changed_input);assert cli_call('feedback.record',changed_input)==changed
 worn=cli_call('wear.list',{'member_id':member})['items'];assert len(worn)==2
 print(json.dumps({'limited_operations':ops,'like':f,'try_another':next,'corrected':corrected,'revoked':revoked,'actual':actual,'changed':changed,'effective_wear_count':len(worn),'feedback_history':mcp('feedback.list',{'member_id':member,'include_revoked':True}),'temporary_data_removed_on_exit':True,'credential_recorded':False},ensure_ascii=False,indent=2))
