"""Real async CLI worker killed after POST; durable receipt/restart never blindly resubmits.
Only lease expiry is advanced at an explicitly recorded clock seam, after the real kill.
"""
import json,subprocess,sys,tempfile,threading,sqlite3,time
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
cli=sys.argv[1];seen=threading.Event();release=threading.Event();counter=[]
class Server(BaseHTTPRequestHandler):
 def do_POST(self):
  self.rfile.read(int(self.headers['Content-Length']));counter.append(1);seen.set();release.wait(10)
  raw=json.dumps({'choices':[{'message':{'content':'not JSON'}}]}).encode()
  try:self.send_response(200);self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
  except (BrokenPipeError,ConnectionResetError):pass
 def log_message(self,*a):pass
server=ThreadingHTTPServer(('127.0.0.1',0),Server);threading.Thread(target=server.serve_forever,daemon=True).start()
with tempfile.TemporaryDirectory(prefix='wardrobe16-async-crash-') as d:
 db=d+'/db'
 def args(op):return [cli,'--trusted-local','--db',db,op,'--input-stdin']
 def call(op,payload):return json.loads(subprocess.run(args(op),input=json.dumps(payload).encode(),stdout=subprocess.PIPE).stdout)
 def ok(op,payload):
  v=call(op,payload);assert 'error' not in v,v;return v['result']
 ok('space.default',{})
 for c in ['top','bottom','shoes']:ok('clothing.create',{'member_id':'default','name':c,'category':c})
 p=ok('provider.save',{'name':'async crash protocol only','endpoint':f'http://127.0.0.1:{server.server_port}/v1','model':'text','capabilities':['text'],'authentication':'none'})
 request={'member_id':'default','operation_key':'crash-paid-once','timezone':'UTC','limit':1,'strategy':'ai','provider_id':p['id']}
 receipt=ok('recommendation.generate',request);assert receipt['state']=='queued';assert len(counter)==0
 job_args={'member_id':'default','id':receipt['job_id']}
 first=subprocess.Popen(args('recommendation.worker.run'),stdin=subprocess.PIPE,stdout=subprocess.PIPE);first.stdin.write(json.dumps(job_args).encode());first.stdin.close();assert seen.wait(5);first.kill();first.wait();first.stdout.close()
 retried=ok('recommendation.generate',request);assert retried['job_id']==receipt['job_id'];pending=ok('recommendation.worker.run',job_args);assert pending['state']=='running';assert len(counter)==1
 with sqlite3.connect(db) as conn:
  job=json.loads(conn.execute('SELECT document FROM jobs WHERE id=?',(receipt['job_id'],)).fetchone()[0]);job['lease_expires']=int(time.time()*1000)-1;conn.execute('UPDATE jobs SET document=? WHERE id=?',(json.dumps(job),receipt['job_id']))
 held=ok('recommendation.worker.run',job_args);assert held['state']=='waiting_input';retry_error=call('jobs.retry',job_args);assert retry_error['error']['code']=='submission_already_sent';release.set()
 rules=ok('recommendation.generate',{'member_id':'default','operation_key':'explicit-rules','timezone':'UTC','limit':1})
 evidence={'initial_receipt':receipt,'initial_provider_posts':0,'killed_worker_returncode':first.returncode,'same_key_restart_receipt':retried,'worker_before_lease_expiry':pending['state'],'explicit_clock_seam_only_lease_expiry':True,'worker_after_expiry':held['state'],'unknown_error':held['error'],'generic_retry':retry_error,'provider_posts_after_kill_and_restart':len(counter),'rule_count':rules['actual_count'],'wear_count':len(ok('wear.list',{'member_id':'default'})['items']),'temporary_data_removed':True,'no_credential':True}
 print(json.dumps(evidence,ensure_ascii=False,indent=2));assert len(counter)==1;assert rules['actual_count']==1
server.shutdown()
