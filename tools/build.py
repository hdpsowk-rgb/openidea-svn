#!/usr/bin/env python3
"""Build and package a self-contained OpenIDEA plugin (Python 3.9+)."""
from pathlib import Path
import argparse, hashlib, json, os, re, shutil, subprocess, zipfile

ROOT=Path(__file__).resolve().parents[1]
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--skip-build',action='store_true');args=parser.parse_args()
    provenance=json.loads((ROOT/'vendor/openidea-plugin/PROVENANCE.json').read_text())
    for source,digest in provenance['files'].items():
        local=source.removeprefix('sdk/rust/openidea-plugin/')
        data=(ROOT/'vendor/openidea-plugin'/local).read_bytes()
        actual=hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
        if actual!=digest:raise SystemExit('Pinned SDK file changed: '+local)
    if not args.skip_build:
        cargo=shutil.which('cargo') or str(Path.home()/'.cargo/bin/cargo')
        env=os.environ.copy();env['PATH']=str(Path(cargo).parent)+os.pathsep+env.get('PATH','')
        subprocess.run([cargo,'build','--locked','--release','--target','wasm32-wasip2'],cwd=ROOT,env=env,check=True)
    wasm=ROOT/'target/wasm32-wasip2/release/openidea_svn.wasm'
    if not wasm.is_file():raise SystemExit('Wasm missing; run the build first.')
    (ROOT/'dist').mkdir(exist_ok=True);shutil.copy2(wasm,ROOT/'dist/plugin.wasm')
    release=ROOT/'release';release.mkdir(exist_ok=True)
    files=[ROOT/'plugin.toml',ROOT/'icon.svg',ROOT/'dist/plugin.wasm',ROOT/'LICENSE',ROOT/'THIRD_PARTY_NOTICES.md',ROOT/'NOTICE.md',ROOT/'README.md']
    files+=sorted((ROOT/'web').glob('*'));files+=sorted((ROOT/'locales').glob('*.json'))
    version=re.search(r'^version\s*=\s*"([^"]+)"', (ROOT/'plugin.toml').read_text(), re.M).group(1)
    for suffix in ['oiplugin','zip']:
        plugin_id=re.search(r'^id\s*=\s*"([^"]+)"', (ROOT/'plugin.toml').read_text(), re.M).group(1)
        path=release/f'{plugin_id}-{version}.{suffix}'
        with zipfile.ZipFile(path,'w',zipfile.ZIP_DEFLATED) as package:
            for file in files:
                info=zipfile.ZipInfo(file.relative_to(ROOT).as_posix(),(2026,11,17,0,0,0));info.compress_type=zipfile.ZIP_DEFLATED;info.external_attr=0o100644<<16
                package.writestr(info,file.read_bytes())
        subprocess.run(['python3',str(ROOT/'tools/check-package.py'),str(path)],check=True)
        print(f'{path}\nSHA256 {hashlib.sha256(path.read_bytes()).hexdigest()}')
if __name__=='__main__':main()
