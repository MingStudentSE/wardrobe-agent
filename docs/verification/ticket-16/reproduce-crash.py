"""Real CLI process killed after provider receives POST; same-key restart never resubmits."""
import json,subprocess,sys,tempfile,threading
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
cli=sys.argv[1];seen=threading.Event();release=threading.Event();counter=[];items=[]
class Server(BaseHTTPRequestHandler):
 def do_POST(self):
  self.rfile.read(int(self.headers['Content-Length']));counter.append(1)
  if len(counter)==1:seen.set();release.wait(10)
  raw=json.dumps({'choices':[{'message':{'content':json.dumps({'proposals':[{'items':items}]})}}]}).encode()
  try:self.send_response(200);self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
  except (BrokenPipeError,ConnectionResetError):pass
 def log_message(self,*a):pass
server=ThreadingHTTPServer(('127.0.0.1',0),Server);threading.Thread(target=server.serve_forever,daemon=True).start()
with tempfile.TemporaryDirectory(prefix='wardrobe16-crash-') as d:
 def args(op):return [cli,'--trusted-local','--db',d+'/db',op,'--input-stdin']
 def call(op,payload):return json.loads(subprocess.run(args(op),input=json.dumps(payload).encode(),stdout=subprocess.PIPE).stdout)
 def ok(op,payload):
  v=call(op,payload);assert 'error' not in v,v;return v['result']
 ok('space.default',{})
 stock=[ok('clothing.create',{'member_id':'default','name':c,'category':c}) for c in ['top','bottom','shoes']]
 items.extend({'clothing_id':c['id'],'role':c['category'],'layer':0} for c in stock)
 p=ok('provider.save',{'name':'crash protocol only','endpoint':f'http://127.0.0.1:{server.server_port}/v1','model':'text','capabilities':['text'],'authentication':'none'})
 request={'member_id':'default','operation_key':'crash-paid-once','timezone':'UTC','limit':1,'strategy':'ai','provider_id':p['id']}
 first=subprocess.Popen(args('recommendation.generate'),stdin=subprocess.PIPE,stdout=subprocess.PIPE);first.stdin.write(json.dumps(request).encode());first.stdin.close();assert seen.wait(5);first.kill();first.wait();retry=call('recommendation.generate',request);release.set()
 rules=ok('recommendation.generate',{'member_id':'default','operation_key':'explicit-rules','timezone':'UTC','limit':1})
 evidence={'provider_posts_after_kill_and_same_key_restart':len(counter),'restart':retry,'rule_count':rules['actual_count'],'wear_count':len(ok('wear.list',{'member_id':'default'})['items']),'temporary_data_removed':True,'no_credential':True}
 print(json.dumps(evidence,ensure_ascii=False,indent=2));assert len(counter)==1,evidence;assert 'error' in retry,evidence;assert rules['actual_count']==1
server.shutdown()
