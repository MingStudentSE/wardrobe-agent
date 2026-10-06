#!/usr/bin/env python3
"""Stage decoder runtime + standalone CLI, and produce a Tauri bundle overlay.

Requires native release CLI and media prefix built for the current target.
Linux release executables must be linked with $ORIGIN/lib and
$ORIGIN/../lib/wardrobe-agent RUNPATH (see media-bundles workflow).
"""
import argparse
import json
import pathlib
import shutil
import subprocess
import sys

p=argparse.ArgumentParser()
p.add_argument('prefix',type=pathlib.Path)
p.add_argument('cli',type=pathlib.Path)
p.add_argument('output',type=pathlib.Path)
p.add_argument('--target',required=True)
p.add_argument('--windows-crt-dir',type=pathlib.Path,help='Licensed MSVC app-local x64 CRT directory')
args=p.parse_args()
output=args.output.resolve();output.mkdir(parents=True,exist_ok=True)
portable=output/'portable';portable.mkdir(exist_ok=True)
sidecar=output/'sidecar';sidecar.mkdir(exist_ok=True)
windows=sys.platform=='win32';linux=sys.platform.startswith('linux');macos=sys.platform=='darwin'
if not windows and not linux and not macos:raise SystemExit('Unsupported native staging platform')
extension='.exe' if windows else ''
cli=portable/f'wardrobe-cli{extension}'
shutil.copy2(args.cli.resolve(strict=True),cli)
runtime=portable if windows else portable/'lib'
runtime.mkdir(exist_ok=True)
resources={};files={}
if windows:
    binaries=sorted((args.prefix/'bin').glob('*.dll'))
    if not binaries or not any('heif' in v.name.lower() for v in binaries) or not any('de265' in v.name.lower() for v in binaries):
        raise SystemExit('Missing native HEIC DLLs')
    for source in binaries:
        destination=runtime/source.name;shutil.copy2(source.resolve(strict=True),destination)
        resources[str(destination)]=destination.name
    if args.windows_crt_dir is None:raise SystemExit('Windows app-local MSVC CRT directory is required')
    crt=list(args.windows_crt_dir.glob('*.dll'))
    if not any(v.name.lower()=='vcruntime140.dll' for v in crt):raise SystemExit('Missing x64 MSVC runtime')
    for source in crt:
        destination=runtime/source.name;shutil.copy2(source,destination);resources[str(destination)]=destination.name
    for source in sorted((args.prefix/'share').glob('*/copyright')):
        destination=portable/'licenses'/source.parent.name/'copyright';destination.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,destination)
        resources[str(destination)]=f'licenses/{source.parent.name}/copyright'
elif linux:
    for name in ('libheif.so.1','libde265.so.0'):
        source=(args.prefix/'lib'/name).resolve(strict=True)
        destination=runtime/name;shutil.copy2(source,destination)
        subprocess.run(['patchelf','--set-rpath','$ORIGIN',str(destination)],check=True)
        files[f'/usr/lib/wardrobe-agent/{name}']=str(destination)
    # Strip every development-prefix lookup from the standalone CLI.
    subprocess.run(['patchelf','--set-rpath','$ORIGIN/lib:$ORIGIN/../lib/wardrobe-agent',str(cli)],check=True)
    for project in ('libheif','libde265'):
        source=args.prefix.parent/project/'COPYING'
        destination=portable/'licenses'/f'{project}-COPYING';destination.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source.resolve(strict=True),destination)
        files[f'/usr/share/doc/wardrobe-agent/{destination.name}']=str(destination)
else:
    for name in ('libheif.1.dylib','libde265.0.dylib'):
        destination=runtime/name
        shutil.copy2((args.prefix/'lib'/name).resolve(strict=True),destination)
        subprocess.run(['install_name_tool','-add_rpath','@loader_path',str(destination)],check=True)
        subprocess.run(['codesign','--force','--sign','-',str(destination)],check=True)
    for rpath in ('@executable_path/lib','@executable_path/../Frameworks'):
        subprocess.run(['install_name_tool','-add_rpath',rpath,str(cli)],check=True)
    subprocess.run(['codesign','--force','--sign','-',str(cli)],check=True)
    for project in ('libheif','libde265'):
        destination=portable/'licenses'/f'{project}-COPYING'
        destination.parent.mkdir(parents=True,exist_ok=True)
        shutil.copy2((args.prefix.parent/project/'COPYING').resolve(strict=True),destination)
        resources[str(destination)]=f'licenses/{destination.name}'
third_party=pathlib.Path(__file__).resolve().parent.parent/'third-party'
for source in sorted(third_party.rglob('*')):
    if source.is_file():
        relative=source.relative_to(third_party)
        destination=portable/'third-party'/relative
        destination.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,destination)
        if linux:files[f'/usr/share/doc/wardrobe-agent/third-party/{relative.as_posix()}']=str(destination)
        else:resources[str(destination)]=f'third-party/{relative.as_posix()}'
shutil.copy2(cli,sidecar/f'wardrobe-cli-{args.target}{extension}')
bundle={'externalBin':[str(sidecar/'wardrobe-cli')]}
if windows:
    bundle.update({'targets':['nsis'],'resources':resources})
elif macos:
    bundle.update({'targets':['app'],'macOS':{'minimumSystemVersion':'13.0','frameworks':[str(runtime/name) for name in ('libheif.1.dylib','libde265.0.dylib')]},'resources':resources})
else:
    bundle.update({'targets':['deb','appimage'],'linux':{'deb':{'files':files},'appimage':{'files':{k.lstrip('/'):v for k,v in files.items()}}}})
(output/'tauri-media.json').write_text(json.dumps({'bundle':bundle},indent=2)+'\n')
(portable/'README.txt').write_text('Run wardrobe-cli capabilities for JSON operations.\nRun wardrobe-cli mcp for the independent stdio MCP server.\nExternal business operations require a client token issued by the local GUI.\nPass tokens only over stdin: CLI operation --client-stdin reads {token,input}; MCP initialize.params.authorization_token authenticates the connection.\nNever put tokens in command arguments, ordinary configuration, reports or logs.\nUse --trusted-local only for explicit human maintenance with local data-directory permissions; do not give this bypass to external clients.\nKeep runtime libraries alongside this executable; no developer prefix is required.\nDecoder sources: https://github.com/strukturag/libheif/tree/v1.23.1 and https://github.com/strukturag/libde265/tree/v1.1.3\nNative decoder libraries use LGPL; replace the corresponding dynamic libraries to relink.\nFull clean-install, signing and upgrade acceptance remains tracked separately.\n')
print(json.dumps({'overlay':str(output/'tauri-media.json'),'portable':str(portable),'target':args.target}))
