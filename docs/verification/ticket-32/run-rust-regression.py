"""Sequential workspace targets; delete only each just-finished owned test executable."""
import json,os,pathlib,re,subprocess,sys
root=pathlib.Path(__file__).resolve().parents[3];report=root/'docs/verification/ticket-32/rust';report.mkdir(exist_ok=True)
packages=sys.argv[1:] or ['wardrobe-core','wardrobe-cli'];rows=[]
for package in packages:
 targets=[['--lib']] if package=='wardrobe-core' else [['--bin','wardrobe-cli']]
 targets += [['--test',p.stem] for p in sorted((root/'crates'/package/'tests').glob('*.rs'))]
 targets += [['--doc']] if package=='wardrobe-core' else []
 for target in targets:
  command=['/Users/links/.cargo/bin/cargo','test','-p',package,*target]
  path=report/f"{package}-{'-'.join(x.strip('-') for x in target)}.log"
  with path.open('w') as output:p=subprocess.run(command,cwd=root,env=os.environ,stdout=output,stderr=subprocess.STDOUT)
  text=path.read_text();totals=[list(map(int,m)) for m in re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;',text)]
  row={'command':command,'exit':p.returncode,'totals':totals,'log':str(path),'removed_terminal_executables':[]}
  for raw in re.findall(r'Running .*?\(([^)]+)\)',text):
   exe=pathlib.Path(raw)
   if not exe.is_absolute():exe=root/exe
   if str(exe).startswith('/tmp/wardrobe-implementation/ticket-09/target/debug/deps/') and exe.exists() and exe.is_file():
    row['removed_terminal_executables'].append({'path':str(exe),'bytes':exe.stat().st_size});exe.unlink()
  rows.append(row);(report/f'{package}-summary.json').write_text(json.dumps(rows,indent=2));print(json.dumps(row),flush=True)
  if p.returncode:sys.exit(p.returncode)
