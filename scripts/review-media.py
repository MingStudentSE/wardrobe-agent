#!/usr/bin/env python3
"""Run real Application API imports and produce an offline side-by-side review.

Usage: review-media.py INPUT_DIR MANIFEST OUTPUT_DIR --cli target/debug/wardrobe-cli
The manifest is a JSON array with file, url/title and license/metadata provenance.
No personal inventory directories are inferred or scanned.
"""
import argparse
import html
import json
import pathlib
import subprocess

parser=argparse.ArgumentParser()
parser.add_argument('input_dir',type=pathlib.Path)
parser.add_argument('manifest',type=pathlib.Path)
parser.add_argument('output_dir',type=pathlib.Path)
parser.add_argument('--cli',type=pathlib.Path,required=True)
parser.add_argument('--files',nargs='*',help='Explicit subset, excluding incomplete downloads')
args=parser.parse_args()
args.output_dir.mkdir(parents=True,exist_ok=True)
database=args.output_dir/'review.sqlite'
def api(operation,data=None):
    run=subprocess.run([str(args.cli.resolve()),'--db',str(database.resolve()),operation,json.dumps(data or {})],capture_output=True,text=True,check=False)
    try:return json.loads(run.stdout)
    except json.JSONDecodeError:return {'error':{'code':'entrypoint_failed','message':run.stderr[:500]}}
api('space.default')
manifest=json.loads(args.manifest.read_text())
records=[]
for sample in manifest:
    filename=sample['file']
    if args.files is not None and filename not in args.files:continue
    source=(args.input_dir/filename).resolve()
    if not source.is_relative_to(args.input_dir.resolve()):raise ValueError('Manifest path leaves sample directory')
    result=api('assets.import',{'member_id':'default','path':str(source)})
    records.append({'sample':sample,'import':result,'review':'pending'})
(args.output_dir/'results.json').write_text(json.dumps(records,ensure_ascii=False,indent=2))
cards=[]
for record in records:
    sample=record['sample'];result=record['import'];title=html.escape(sample.get('title',sample['file']))
    if 'error' in result:
        cards.append(f'<section><h2>{title}</h2><pre>{html.escape(json.dumps(result,ensure_ascii=False))}</pre></section>');continue
    image=result['result'];display=image['display'];original=image['original']
    original_url=(args.input_dir/sample['file']).resolve().as_uri();display_url=pathlib.Path(display['local_path']).resolve().as_uri()
    source=html.escape(sample.get('url',''),quote=True)
    provenance=html.escape(json.dumps(sample.get('metadata',sample.get('license',{})),ensure_ascii=False,indent=2))
    cards.append(f'<section><h2>{title}</h2><p><a href="{source}">Source / license provenance</a> · {original["bytes"]:,} → {display["bytes"]:,} bytes · {display["width"]}×{display["height"]} · q82 · manual review pending</p><div class="pair"><figure><img src="{original_url}" alt="Original"><figcaption>Original (local review only)</figcaption></figure><figure><img src="{display_url}" alt="Compressed display"><figcaption>Normalized WebP display</figcaption></figure></div><details><summary>Provenance</summary><pre>{provenance}</pre></details></section>')
document='<!doctype html><html lang="en"><meta charset="utf-8"><title>Garment compression review</title><style>body{font:16px system-ui;max-width:1600px;margin:32px auto;padding:16px;background:#f7f5ef;color:#262923}section{margin:48px 0}.pair{display:grid;grid-template-columns:1fr 1fr;gap:24px}figure{margin:0}img{width:100%;height:700px;object-fit:contain;background:white}pre{white-space:pre-wrap;overflow-wrap:anywhere}@media(max-width:700px){.pair{grid-template-columns:1fr}}</style><h1>Garment compression review</h1><p>Actual imports; count and manual reviews remain pending until recorded. Check white detail, weave, fine stripes, prints, orientation and color. Synthetic fixtures do not count toward 20 garment samples.</p>'+''.join(cards)+'</html>'
(args.output_dir/'comparison.html').write_text(document)
print(json.dumps({'review_file':str((args.output_dir/'comparison.html').resolve()),'samples':len(records),'successful_imports':sum('result' in r['import'] for r in records),'manual_review':'pending'},ensure_ascii=False))
