#!/usr/bin/env python3
"""Compare extra native views with isolated competitor processes; cat stays literal."""
import argparse
import csv
import io
import gzip
import hashlib
import importlib.metadata
import json
import os
import re
from pathlib import Path
import statistics
import subprocess
import sys
import time
import tiktoken

root=Path(__file__).resolve().parents[2]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--work', type=Path, required=True)
parser.add_argument('--before', type=Path)
parser.add_argument('--rtk', type=Path)
parser.add_argument('--built-oracle', type=Path)
parser.add_argument('--publish', action='store_true')
args=parser.parse_args()
work=args.work.resolve()
work.mkdir(parents=True, exist_ok=False)
published=json.loads(gzip.decompress((root/'bench/headroom-extra/corpus.json.gz').read_bytes()))
published.append(json.loads(gzip.decompress((root/'bench/headroom-extra/npm-lock.json.gz').read_bytes())))
lm=root/'target/release/lm-resizer'
rtk=args.rtk.resolve() if args.rtk else root/'target/rtk-parity/bin/rtk'
assert importlib.metadata.version('headroom-ai') == '0.39.1'
pin=json.loads((root/'bench/rtk-parity/reference.json').read_text())
if args.built_oracle:
    receipt=json.loads(args.built_oracle.read_text())
    assert receipt['source_archive_sha256']==pin['source_archive_sha256']
    pin['binary_sha256']=receipt['binary_sha256']
assert hashlib.sha256(rtk.read_bytes()).hexdigest() == pin['binary_sha256']
(work/"home").mkdir()
env=dict(os.environ, HOME=str(work/"home"), LM_RESIZER_STATE_DIR=str(work/'state'), LM_RESIZER_STORE=str(work/'ccr.sqlite'),
         XDG_DATA_HOME=str(work/'data'), XDG_CONFIG_HOME=str(work/'config'),
         XDG_CACHE_HOME=str(work/'cache'), HEADROOM_CCR_SQLITE_PATH=str(work/'hr.sqlite'),
         RTK_TELEMETRY_DISABLED='1', RTK_SUPPRESS_HOOK_WARNING='1', LM_RESIZER_TRACKING='0')
enc=tiktoken.get_encoding('o200k_base')
count=lambda s:len(enc.encode(s,disallowed_special=()))
def invoke(args,data=None):
    start=time.perf_counter()
    p=subprocess.run(args,input=data,capture_output=True,text=True,env=env,cwd=work,timeout=120)
    if p.returncode: raise RuntimeError(p.stderr)
    return p.stdout,(time.perf_counter()-start)*1000
rows=[]
corpus=[]
for name in ['python-distributions','health-log','rust-source','npm-lock']:
    raw=next(case['raw'] for case in published if case['id']==name)
    corpus.append(dict(id=name,raw=raw,source='installed metadata' if name=='python-distributions' else
                       'controlled Python logging workload' if name=='health-log' else 'src/lossless_filters.rs'))
    lm_cmd=[str(lm),'exec','--json','--','python3','-c','import sys; sys.stdout.write(sys.stdin.read())']
    # RTK has no rewrite for this producer: explicit proxy passes its output.
    ref_cmd=[str(rtk),'proxy','python3','-c','import sys; sys.stdout.write(sys.stdin.read())']
    data=raw
    hr_cmd=['sh',str(root/'bench/headroom_once.sh'),sys.executable]
    if name=='npm-lock':
        hr_cmd=[sys.executable,'-c','import sys; from headroom.transforms.smart_crusher import SmartCrusher,SmartCrusherConfig; sys.stdout.write(SmartCrusher(config=SmartCrusherConfig(lossless_only=True)).compact_document_json(sys.stdin.read()))']
    elif name=='health-log':
        hr_cmd=[sys.executable,'-c','import sys; from headroom.transforms.log_compressor import LogCompressor; sys.stdout.write(LogCompressor().compress(sys.stdin.read()).compressed)']
    elif name=='rust-source':
        (work/'source.rs').write_text(raw)
        # Extra view only for an unsupported producer; cat/read stay exact.
        ref_cmd=[str(rtk),'read','source.rs']
        hr_cmd=[sys.executable,'-c','import sys; from headroom.transforms.code_compressor import compress_code; sys.stdout.write(compress_code(sys.stdin.read(),language="rust"))']
        data=raw
    times={k:[] for k in ['lm','rtk','headroom']}
    for _ in range(3):
        ref,ms=invoke(ref_cmd,data);times['rtk'].append(ms)
        rendered,ms=invoke(lm_cmd,data);times['lm'].append(ms)
        report=json.loads(rendered)
        hr,ms=invoke(hr_cmd,raw);times['headroom'].append(ms)
    default_hr,_=invoke(['sh',str(root/'bench/headroom_once.sh'),sys.executable],raw)
    (work/(name+'.headroom-default')).write_text(default_hr)
    before_tokens=None
    if args.before:
        before,_=invoke([str(args.before.resolve()), *lm_cmd[1:]],raw)
        before_tokens=count(json.loads(before)['output'])
    view=report['output']
    assert count(view)<count(raw), 'extra does not reduce tokens'
    key=report['tee_hint'].removeprefix('[raw: ').removesuffix(']')
    recovered,_=invoke([str(lm),'tee','read',key])
    assert recovered==raw
    if name=='health-log':
        assert 'ERROR request_failed request_id=42 reason=database_unavailable' in view
        assert view.count('[repeat 499 more]')==2
    if name=='python-distributions':
        # Restore every typed record from the rendered table, no row sampling.
        if view.startswith('CSV strings['):
            csv_rows=list(csv.reader(io.StringIO(view.split('\n',1)[1])))
            length=len(json.loads(raw))
            restored=[dict(zip(csv_rows[0],row)) for row in csv_rows[1:length+1]]
        else:
            table=json.JSONDecoder().raw_decode(view.split('\n',1)[1])[0]
            restored=[dict(zip(table['columns'],row)) for row in table['rows']]
        assert restored==json.loads(raw)
    if name=='npm-lock':
        assert json.JSONDecoder().raw_decode(view)[0]==json.loads(raw)
        # The document walker replaces one nested array with a CSV+schema
        # string. Decode that explicit representation before comparing facts.
        def restore_headroom(value):
            if isinstance(value, dict):
                return {k: restore_headroom(v) for k,v in value.items()}
            if isinstance(value, list):
                return [restore_headroom(v) for v in value]
            if isinstance(value, str):
                match=re.fullmatch(r"\[(\d+)\]\{([^\n]+)\}\n(.*)", value, re.S)
                if match:
                    fields=match[2].split(',')
                    assert all(f.endswith(':string') for f in fields)
                    records=list(csv.reader(io.StringIO(match[3])))
                    assert len(records)==int(match[1])
                    return [dict(zip([f.removesuffix(':string') for f in fields], row)) for row in records]
            return value
        assert restore_headroom(json.loads(hr))==json.loads(raw)
    if name=='rust-source':
        assert view.startswith('Rust outline')
        direct,_=invoke([str(lm),'exec','--json','--','cat','source.rs'])
        direct=json.loads(direct)
        assert direct['output'].encode()[:direct['filtered_bytes']]==ref.encode()
    for label,text in [('lm',view),('rtk',ref),('headroom',hr)]:
        (work/(name+'.'+label)).write_text(text)
    rows.append(dict(id=name,raw_tokens=count(raw),rtk_tokens=count(ref),
                     lm_tokens=count(view),before_tokens=before_tokens,headroom_tokens=count(hr),headroom_default_tokens=count(default_hr),tee_verified=True,
                     headroom_mode='CodeAwareCompressor' if name=='rust-source' else
                     'LogCompressor' if name=='health-log' else
                     'SmartCrusher.compact_document_json(lossless_only=True)' if name=='npm-lock' else 'compress()',
                     milliseconds={k:statistics.median(v) for k,v in times.items()}))
result=dict(headroom='0.39.1',rtk=pin['version'],lm_sha256=hashlib.sha256(lm.read_bytes()).hexdigest(),
            tokenizer='tiktoken 0.14.0 o200k_base',repetitions=3,cases=rows)
(work/'results.json').write_text(json.dumps(result,indent=2)+'\n')
# Replay the pinned public corpus; never replace it with machine-dependent metadata.
if args.publish:
    (root/'bench/headroom-extra/results.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(rows,indent=2))
