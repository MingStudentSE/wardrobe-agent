"""Temporary installation: actual stdin CLI and persistent MCP maintenance scopes."""
import json,pathlib,subprocess,sys,tempfile
CLI=str(pathlib.Path(sys.argv[1]).resolve())
def cli(db,name,args,token=None):
 argv=[CLI,'--db',str(db)]
 if token:argv += [name,'--client-stdin'];payload={'token':token,'input':args}
 else:argv += ['--trusted-local',name,'--input-stdin'];payload=args
 p=subprocess.run(argv,input=json.dumps(payload),text=True,capture_output=True)
 return json.loads(p.stdout)
class MCP:
 def __init__(self,db,token):
  self.p=subprocess.Popen([CLI,'--db',str(db),'mcp'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True);self.id=0
  response=self.send('initialize',{'authorization_token':token});assert 'error' not in response,response
 def send(self,method,params):
  self.id+=1;self.p.stdin.write(json.dumps({'jsonrpc':'2.0','id':self.id,'method':method,'params':params})+'\n');self.p.stdin.flush();return json.loads(self.p.stdout.readline())
 def call(self,name,args):
  response=self.send('tools/call',{'name':name,'arguments':args});assert 'error' not in response,response
  return json.loads(response['result']['content'][0]['text'])
 def close(self):self.p.stdin.close();self.p.wait(timeout=10);assert self.p.returncode==0
with tempfile.TemporaryDirectory(prefix='wardrobe32-public-scope-') as raw:
 root=pathlib.Path(raw);db=root/'source'/'wardrobe.sqlite'
 member=cli(db,'member.list',{})['result']['items'][0]['member_id']
 fact=cli(db,'clothing.create',{'member_id':member,'name':'scope fixture','category':'top'})['result']
 queued=cli(db,'backup.create',{'directory':str(root/'package')})['result']
 def grant(operations,member_ids,mode='limited'):
  return cli(db,'client.authorize',{'name':'temporary verifier','mode':mode,'operations':operations,'member_ids':member_ids,'expires_at':None})['result']['token']
 token=grant([], [member], 'full');m=MCP(db,token)
 args={'backup.create':{'directory':str(root/'forbidden')},'backup.preview':{'directory':str(root/'package')},'backup.restore':{'directory':str(root/'package'),'confirm_overwrite':True},'data_directory.move':{'directory':str(root/'forbidden-move')},'maintenance.get':{'id':queued['id']},'maintenance.list':{},'maintenance.cancel':{'id':queued['id']},'maintenance.retry':{'id':queued['id']},'maintenance.worker.run':{'id':queued['id']}}
 for name,input in args.items():
  assert cli(db,name,input,token)['error']['code']=='member_denied',name
  assert m.call(name,input)['error']['code']=='member_denied',name
 assert 'error' in m.call('jobs.get',{'member_id':member,'id':queued['id']})
 assert m.call('jobs.list',{'member_id':member})['items']==[]
 m.close()
 token=grant(['maintenance.get','maintenance.list','maintenance.worker.run'],None);m=MCP(db,token)
 assert m.call('maintenance.get',{'id':queued['id']})['error']['code']=='scope_denied'
 assert m.call('maintenance.worker.run',{'id':queued['id']})['error']['code']=='scope_denied'
 assert m.call('maintenance.list',{})['items']==[];m.close()
 token=grant([],None,'full');m=MCP(db,token)
 def run(name,input):
  submitted=m.call(name,input);assert submitted['state']=='queued' and submitted['job_id'],submitted
  receipt=m.call('maintenance.worker.run',{'id':submitted['job_id']});assert receipt['state']=='succeeded',receipt
  return receipt['result']
 receipt=m.call('maintenance.worker.run',{'id':queued['id']});assert receipt['state']=='succeeded',receipt
 run('backup.create',{'directory':str(root/'mcp-package')})
 assert run('backup.preview',{'directory':str(root/'package')})['valid']
 # Restore removes local client grants; initialize a fresh local grant afterward.
 run('backup.restore',{'directory':str(root/'package'),'confirm_overwrite':True})
 assert m.call('clothing.get',{'member_id':member,'id':fact['id']})['error']['code']=='unauthorized';m.close()
 token=grant([],None,'full');m=MCP(db,token)
 assert m.call('clothing.get',{'member_id':member,'id':fact['id']})==fact
 run('data_directory.move',{'directory':str(root/'moved')})
 assert m.call('clothing.get',{'member_id':member,'id':fact['id']})==fact
 assert cli(root/'moved'/'wardrobe.sqlite','clothing.get',{'member_id':member,'id':fact['id']})['result']==fact
 m.close()
 print(json.dumps({'limited_member_cli_and_mcp_denied':list(args),'whole_space_jobs_absent_from_member_jobs':True,'maintenance_control_requires_original_business_scope':True,'whole_space_mcp_backup_preview_restore_move':True,'restore_revokes_old_session_grant':True,'old_mcp_and_new_cli_follow_directory':True}))
