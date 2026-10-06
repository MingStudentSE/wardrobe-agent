"""Actual public weather and configured local licensed text model; synthetic wardrobe only."""
import json,sys,os,subprocess,tempfile,threading,urllib.request
from http.server import BaseHTTPRequestHandler,HTTPServer
cli=sys.argv[1];responses=[]
class Proxy(BaseHTTPRequestHandler):
 def do_POST(self):
  body=self.rfile.read(int(self.headers['Content-Length']))
  req=urllib.request.Request('http://127.0.0.1:18086'+self.path,data=body,headers={'Content-Type':'application/json'})
  with urllib.request.urlopen(req,timeout=90) as r:raw=r.read()
  responses.append({'request':json.loads(body),'response':json.loads(raw)})
  self.send_response(200);self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
 def log_message(self,*args):pass
server=HTTPServer(('127.0.0.1',18087),Proxy);threading.Thread(target=server.serve_forever,daemon=True).start()
with tempfile.TemporaryDirectory(prefix='wardrobe16-real-') as d:
 def call(op,input):
  p=subprocess.run([cli,'--trusted-local','--db',d+'/db',op,'--input-stdin'],input=json.dumps(input).encode(),stdout=subprocess.PIPE)
  body=json.loads(p.stdout);assert 'error' not in body,body;return body['result']
 call('space.default',{})
 call('weather.configure',{'member_id':'default','mode':'automatic','location':'Shanghai public city example','latitude':31.23,'longitude':121.47,'usage':'noncommercial'})
 weather=call('weather.refresh',{'member_id':'default'})
 stock=[call('clothing.create',{'member_id':'default','name':name,'category':cat}) for name,cat in [('white cotton shirt','top'),('blue straight jeans','bottom'),('white sneakers','shoes'),('brown coat','outerwear'),('black knit top','top'),('beige trousers','bottom'),('black loafers','shoes')]]
 provider=call('provider.save',{'name':'temporary real local SmolVLM','endpoint':'http://127.0.0.1:18087/v1','model':os.environ.get('WARDROBE_ACTUAL_MODEL','wardrobe-smolvlm'),'capabilities':['text'],'authentication':'none'})
 result=call('recommendation.generate',{'member_id':'default','operation_key':'real-model','timezone':'Asia/Shanghai','limit':1,'strategy':'ai','provider_id':provider['id'],'weather':{'mode':'automatic'},'json_output':True,'fixed_ids':[stock[3]['id']],'output_format':'llama_cpp_schema'})
 wear=call('wear.list',{'member_id':'default'})
 assert not wear['items']
 print(json.dumps({'actual_public_weather':weather,'actual_recommendation':result,'raw_local_model_calls':responses,'wear_count':0,'temporary_data_removed':True,'no_paid_provider':True},ensure_ascii=False,indent=2))
server.shutdown()
