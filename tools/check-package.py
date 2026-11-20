#!/usr/bin/env python3
"""Check our release layout; OpenIDEA itself validates the component API at install."""
from pathlib import PurePosixPath
import json,re,sys,zipfile
with zipfile.ZipFile(sys.argv[1]) as package:
    names=package.namelist()
    if len(names)!=len(set(names)):raise SystemExit('Duplicate archive entries')
    for name in names:
        path=PurePosixPath(name)
        if path.is_absolute() or '..' in path.parts or '\\' in name:raise SystemExit('Unsafe archive path: '+name)
    for name in ['plugin.toml','dist/plugin.wasm','web/index.html','web/app.js','web/model.js','web/style.css','locales/zh-CN.json','icon.svg','LICENSE','NOTICE.md']:
        if name not in names:raise SystemExit('Missing package file: '+name)
    manifest=package.read('plugin.toml').decode()
    if not re.search(r'^id\s*=\s*"hdpsowk-rgb\.svn"',manifest,re.M):raise SystemExit('Wrong plugin id')
    if package.read('dist/plugin.wasm')[:8]!=b'\0asm\x0d\0\x01\0':raise SystemExit('Expected a WebAssembly component')
    json.loads(package.read('locales/zh-CN.json'))
    print('Package layout OK:',sys.argv[1])
