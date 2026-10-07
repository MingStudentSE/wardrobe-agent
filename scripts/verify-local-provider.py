#!/usr/bin/env python3
"""Opt-in real local inference proof; starts no server and reads no account secrets.
Requires a prestarted loopback OpenAI-compatible text/vision server and real CLI.
Temporary SQLite + disposable client token are deleted; token never enters argv/report.
"""
import argparse, base64, json, pathlib, subprocess, tempfile, time, urllib.parse, urllib.request

parser = argparse.ArgumentParser()
parser.add_argument('--cli', required=True)
parser.add_argument('--endpoint', default='http://127.0.0.1:18081/v1')
parser.add_argument('--model', default='wardrobe-smolvlm')
parser.add_argument('--output', required=True)
args = parser.parse_args()
url = urllib.parse.urlsplit(args.endpoint)
if url.scheme != 'http' or url.hostname != '127.0.0.1' or url.username or url.password or url.query:
    parser.error('This verification only accepts a credential-free 127.0.0.1 HTTP server.')
root = pathlib.Path(__file__).resolve().parents[1]
image = base64.b64encode((root/'crates/wardrobe-core/src/validation-image.png').read_bytes()).decode()
report = {'endpoint': args.endpoint, 'model': args.model, 'authentication': 'none', 'warmup': {}, 'cli': {}, 'mcp': {}}
with urllib.request.urlopen(args.endpoint+'/models', timeout=20) as response:
    report['models'] = json.load(response)
# Actual warmup model responses, same content/max_tokens as Application's verifier.
for capability in ('text','vision'):
    content = 'Reply OK.' if capability == 'text' else [
        {'type':'text','text':'Briefly describe the shapes and colors in this test image.'},
        {'type':'image_url','image_url':{'url':'data:image/png;base64,'+image,'detail':'low'}}]
    body = {'model':args.model,'messages':[{'role':'user','content':content}], 'max_tokens':1 if capability=='text' else 32}
    start = time.monotonic()
    request = urllib.request.Request(args.endpoint+'/chat/completions', data=json.dumps(body).encode(), headers={'Content-Type':'application/json'})
    with urllib.request.urlopen(request, timeout=120) as response:
        result = json.load(response)
    assert result['choices'][0]['message']['content'].strip()
    report['warmup'][capability] = {'elapsed_seconds': round(time.monotonic()-start,3), 'response':result}
with tempfile.TemporaryDirectory(prefix='wardrobe-real-local-') as directory:
    db = str(pathlib.Path(directory)/'wardrobe.sqlite3')
    def trusted(operation, value):
        result = subprocess.run([args.cli,'--trusted-local','--db',db,operation,'--input-stdin'],input=json.dumps(value),capture_output=True,text=True,check=True)
        decoded = json.loads(result.stdout)
        assert 'result' in decoded, decoded
        return decoded['result']
    trusted('space.default',{})
    provider = trusted('provider.save',{'name':'SmolVLM real local proof','endpoint':args.endpoint,'model':args.model,'authentication':'none','capabilities':['text','vision']})
    report['saved_provider'] = provider
    token = trusted('client.authorize',{'name':'disposable validator','mode':'limited','operations':['provider.validate'],'member_ids':['default'],'expires_at':None})['token']
    for capability in ('text','vision'):
        value = {'id':provider['id'],'capability':capability}
        start = time.monotonic()
        output = subprocess.run([args.cli,'--db',db,'provider.validate','--client-stdin'],input=json.dumps({'token':token,'input':value}),capture_output=True,text=True,check=True)
        decoded = json.loads(output.stdout)
        assert 'result' in decoded, decoded
        assert decoded['result']['verified_capabilities']==[capability]
        report['cli'][capability]={'elapsed_seconds':round(time.monotonic()-start,3),'result':decoded['result']}
        request = {'jsonrpc':'2.0','id':1,'method':'tools/call','params':{'authorization_token':token,'name':'provider.validate','arguments':value}}
        start = time.monotonic()
        output = subprocess.run([args.cli,'--db',db,'mcp'],input=json.dumps(request)+'\n',capture_output=True,text=True,check=True)
        decoded = json.loads(output.stdout)
        result = json.loads(decoded['result']['content'][0]['text'])
        assert result['verified_capabilities']==[capability], result
        report['mcp'][capability]={'elapsed_seconds':round(time.monotonic()-start,3),'result':result}
    report['temporary_database_removed'] = True
pathlib.Path(args.output).write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
print('Real inference verified: CLI text/vision and MCP text/vision; disposable database removed.')
