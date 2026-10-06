"""Actual temporary OS keys + two CLI installations + isolated filesystem boundary.
Run with WARDROBE_CLI pointing to the same-source compiled binary. No keys logged.
"""
import base64,json,os,pathlib,struct,subprocess,tempfile,zlib
CLI=os.environ['WARDROBE_CLI']
def call(folder,op,body=None,mode='trusted',ok=True):
 args=[CLI]+(['--trusted-local'] if mode=='trusted' else [])+[op,'--input-stdin' if mode=='trusted' else '--client-stdin']
 proc=subprocess.run(args,input=json.dumps(body or {}),text=True,capture_output=True,env=dict(os.environ,WARDROBE_DATA_DIR=str(folder)))
 result=json.loads(proc.stdout)
 if ok:assert proc.returncode==0,(op,result.get('error',{}).get('code'))
 else:assert proc.returncode!=0
 return result.get('result',result.get('error'))
def chunk(name,value):return struct.pack('>I',len(value))+name+value+struct.pack('>I',zlib.crc32(name+value))
with tempfile.TemporaryDirectory(prefix='wardrobe-export-cli-') as root:
 root=pathlib.Path(root);owner=root/'owner';peer=root/'peer';external=root/'external';external.mkdir()
 try:
  call(owner,'encryption.create',{'password':'temporary CLI export password','device_name':'CLI owner'})
  offer=call(owner,'encryption.pair.offer',{'device_name':'CLI paired reader'})
  accepted=call(peer,'encryption.pair.accept',{'invitation':offer['invitation'],'pairing_secret':offer['pairing_secret'],'password':'temporary paired CLI password'})
  confirmed=call(owner,'encryption.pair.confirm',{'receipt':accepted['receipt']})
  call(peer,'encryption.device.apply',{'manifest':confirmed['manifest']})
  garment=call(owner,'clothing.create',{'member_id':'default','name':'private-cli-wardrobe-fact','category':'top','notes':'private-cli-note'})
  original=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',64,64,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\0'+bytes([33,71,96])*64)*64))+chunk(b'IEND',b'')
  source=root/'private-original-camera.png';source.write_bytes(original)
  asset=call(owner,'assets.import',{'member_id':'default','path':str(source)})
  call(owner,'assets.adopt',{'member_id':'default','clothing_id':garment['id'],'asset_id':asset['id']})
  look=call(owner,'outfit.create',{'member_id':'default','name':'CLI immutable history','items':[{'clothing_id':garment['id'],'role':'top','layer':0}]})
  call(owner,'wear.record',{'member_id':'default','operation_key':'CLI history','local_date':'2026-10-01','timezone':'Asia/Shanghai','source_outfit_id':look['id'],'source_version':1})
  call(owner,'assets.detach',{'member_id':'default','clothing_id':garment['id'],'asset_id':asset['id']})
  exported=call(owner,'encryption.package.export',{'directory':str(external)});package=pathlib.Path(exported['path'])
  for child in package.iterdir():
   data=child.read_bytes()
   for private in [b'private-cli-wardrobe-fact',b'private-cli-note',b'private-original-camera.png',str(owner).encode(),original]:assert private not in data
  inspected=call(peer,'encryption.package.read',{'path':str(package)})
  assert inspected['snapshot']['clothing'][0]['name']==garment['name']
  assert inspected['snapshot']['wear_events'][0]['source_outfit']['version']==1
  assert inspected['assets'][0]['id']==asset['id']
  assert 'local_path' not in json.dumps(inspected) and 'original' not in json.dumps(inspected)
  assert call(peer,'clothing.list',{'member_id':'default'})['items']==[]
  client=call(owner,'client.authorize',{'name':'CLI full package denial','mode':'full','operations':[],'member_ids':None,'expires_at':None})
  for operation in ['encryption.package.export','encryption.package.read']:
   assert call(owner,operation,{'token':client['token'],'input':{'directory':str(external),'path':str(package)}},mode='client',ok=False)['code']=='scope_denied'
  # Actual MCP tools list cannot expose privileged encryption package tools.
  requests='\n'.join(json.dumps(v) for v in [{'jsonrpc':'2.0','id':1,'method':'initialize','params':{'authorization_token':client['token']}},{'jsonrpc':'2.0','id':2,'method':'tools/list','params':{}}])+'\n'
  proc=subprocess.run([CLI,'mcp'],input=requests,text=True,capture_output=True,env=dict(os.environ,WARDROBE_DATA_DIR=str(owner)))
  assert proc.returncode==0
  messages=[json.loads(v) for v in proc.stdout.splitlines() if v.strip()]
  tools=next(v['result']['tools'] for v in messages if v.get('id')==2)
  assert not any(t['name'].startswith('encryption.') for t in tools)
  print(json.dumps({'paired_installations':2,'external_files':len(list(package.iterdir())),'display_assets':len(inspected['assets']),'original_or_plaintext_found':False,'peer_facts_imported':False,'full_client_package_operations_denied':2,'mcp_encryption_tools':0}))
 finally:
  for directory in [peer,owner]:
   if directory.exists():call(directory,'encryption.unpair')
