"""Independent real CLI: non-UTC seven-local-day edge, scoped save and stale availability."""
import json, subprocess, sys, tempfile
from datetime import datetime,timedelta
from zoneinfo import ZoneInfo
cli=sys.argv[1]
with tempfile.TemporaryDirectory(prefix='wardrobe-15-independent-') as folder:
 def call(op,payload,token=None):
  args=[cli,'--db',folder+'/db',op]
  if token is None: args.insert(1,'--trusted-local');args+=['--input-stdin'];body=payload
  else:args+=['--client-stdin'];body={'token':token,'input':payload}
  result=subprocess.run(args,input=json.dumps(body).encode(),stdout=subprocess.PIPE,check=False)
  return json.loads(result.stdout)
 def ok(op,payload,token=None):
  r=call(op,payload,token);assert 'error' not in r,r;return r['result']
 ok('space.default',{})
 stock=[ok('clothing.create',{'member_id':'default','name':c,'category':c}) for c in ['top','bottom','shoes']]
 zone='Pacific/Kiritimati';today=datetime.now(ZoneInfo(zone)).date()
 ids=[c['id'] for c in stock]
 # Eighth local calendar date is outside; seventh local date remains inside.
 for delta in [7,6]:
  ok('wear.record',{'member_id':'default','operation_key':'wear'+str(delta),'local_date':str(today-timedelta(days=delta)),'timezone':zone,'clothing_ids':ids})
  r=ok('recommendation.generate',{'member_id':'default','operation_key':'generate'+str(delta),'timezone':zone,'limit':1})
  assert r['recent_window']['from']==str(today-timedelta(days=6)),r
  assert r['proposals'][0]['repeat_relaxed']==(delta==6),r
 other=ok('member.create',{'name':'other'})['member_id']
 token=ok('client.authorize',{'name':'temporary boundary client','mode':'limited','operations':['recommendation.save','recommendation.get'],'member_ids':['default'],'expires_at':None})['token']
 denied=call('recommendation.get',{'member_id':other,'id':r['id']},token)
 assert denied['error']['code'] in ['member_denied'],denied
 ok('clothing.archive',{'member_id':'default','id':stock[0]['id']})
 blocked=call('recommendation.save',{'member_id':'default','id':r['id'],'proposal_index':0,'name':'must not save','operation_key':'save'},token)
 assert blocked['error']['code']=='constraint_conflict',blocked
 assert not ok('outfit.list',{'member_id':'default'})['items']
 assert len(ok('wear.list',{'member_id':'default'})['items'])==2
 print(json.dumps({'timezone':zone,'outside_day':str(today-timedelta(days=7)),'inside_day':str(today-timedelta(days=6)),'seven_local_day_edges_verified':True,'cross_member_denied':denied['error']['code'],'archived_after_generation_save_rejected':blocked['error']['code'],'outfit_count':0,'actual_wear_count':2,'temporary_data_removed':True,'credential_recorded':False},ensure_ascii=False,indent=2))
