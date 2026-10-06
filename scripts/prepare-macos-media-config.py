#!/usr/bin/env python3
"""Create a Tauri config overlay embedding the isolated HEIC decoder libraries."""
import argparse
import json
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('prefix', type=Path)
parser.add_argument('output', type=Path)
args = parser.parse_args()
for name in ('libheif.1.dylib', 'libde265.0.dylib'):
    (args.prefix / 'lib' / name).resolve(strict=True)
# Preserve the install names used by @rpath, rather than resolving their versioned filenames.
frameworks = [str((args.prefix / 'lib' / name).absolute()) for name in ('libheif.1.dylib', 'libde265.0.dylib')]
args.output.write_text(json.dumps({'bundle': {'macOS': {'frameworks': frameworks}}}, indent=2) + '\n')
