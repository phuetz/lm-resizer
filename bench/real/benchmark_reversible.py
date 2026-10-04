#!/usr/bin/env python3
"""Measure native reversible folds against pinned comparison tools."""
import argparse,gzip,hashlib,json,os,subprocess,time
from pathlib import Path
import tiktoken
p=argparse.ArgumentParser();p.add_argument('--work',type=Path,required=True);a=p.parse_args()
root=Path(__file__).resolve().parents[2];work=a.work.resolve();work.mkdir(parents=True,exist_ok=False)
lm=root/'target/release/lm-resizer';rtk=root/'target/rtk-parity/oracle-build/release/rtk'
receipt=json.loads((root/'target/rtk-parity/oracle-build/receipt.json').read_text())
assert hashlib.sha256(rtk.read_bytes()).hexdigest()==receipt['binary_sha256']
assert receipt['source_archive_sha256']==json.loads((root/'bench/rtk-parity/reference.json').read_text())['source_archive_sha256']
enc=tiktoken.get_encoding('o200k_base');count=lambda s:len(enc.encode(s,disallowed_special=()))
env=dict(os.environ,HOME=str(work/'home'),XDG_DATA_HOME=str(work/'data'),XDG_CONFIG_HOME=str(work/'config'),LM_RESIZER_STATE_DIR=str(work/'state'),LM_RESIZER_STORE=str(work/'ccr.sqlite'),RTK_TELEMETRY_DISABLED='1',RTK_SUPPRESS_HOOK_WARNING='1',LM_RESIZER_TRACKING='0')
(work/'home').mkdir()
corpus=json.loads(gzip.decompress((root/'bench/rtk-parity/corpus.json.gz').read_bytes()))
# Controlled Windows-shaped producers supplement, not relabel, real captures.
corpus.extend([
 dict(id='controlled-windows-find', stdout=''.join(f'C:\\workspace avec espace\\src\\module\\file{n}.rs\n' for n in range(100))),
 dict(id='controlled-windows-grep', stdout=''.join(f'C:\\workspace avec espace\\src\\module\\file.rs:{n}:value: detail\n' for n in range(1,101))),
])
rows=[]
for case in corpus:
    kind='paths' if case['id'].endswith('-find') else 'search' if case['id'] in ['TypeScript-grep','compare-grep','controlled-windows-grep'] else 'diff' if case['id'].endswith('-diff') else None
    if not kind or case['stdout'].count('\n')<5:continue
    raw=case['stdout']
    producer=['python3','-c','import sys; sys.stdout.write(sys.stdin.read())']
    def run(cmd):return subprocess.run(cmd,input=raw,text=True,capture_output=True,env=env,check=True).stdout
    reference=run([str(rtk),'proxy',*producer]);assert reference==raw
    report=json.loads(run([str(lm),'exec','--json','--',*producer]));view=report['output'];body=view.encode()[:report['filtered_bytes']].decode()
    expanded=subprocess.run([lm,'expand'],input=body,text=True,capture_output=True,env=env,check=True).stdout
    assert kind=='diff' or expanded==raw,case['id']
    key=report['tee_hint'].removeprefix('[raw: ').removesuffix(']')
    recovered=subprocess.check_output([lm,'tee','read',key],env=env).decode();assert recovered==raw
    from headroom.transforms.lossless_compaction import compact_lossless
    hr=compact_lossless(raw,kind)
    # Record the public return shape explicitly; specialised path is not the default API.
    if not isinstance(hr,str):raise TypeError(type(hr))
    rows.append(dict(id=case['id'],mode=kind,raw_tokens=count(raw),rtk_tokens=count(reference),lm_view_tokens=count(body),lm_total_tokens=count(view),headroom_specialized_tokens=count(hr),exact_inverse=kind!='diff',tee_exact=True,filter=report['filter']))
# A controlled repeated window, not a claim about real session frequency.
log=next(c['stdout'] for c in corpus if c['id']=='ripgrep-log')
messages=[dict(role='tool',content=log),dict(role='tool',content=log)]
raw=json.dumps(messages,separators=(',',':'),ensure_ascii=False)
view=subprocess.run([lm,'dedup'],input=raw,text=True,capture_output=True,env=env,check=True).stdout
expanded=subprocess.run([lm,'expand'],input=view,text=True,capture_output=True,env=env,check=True).stdout
assert json.loads(expanded)==messages
key=hashlib.sha256(raw.encode()).hexdigest()
assert subprocess.check_output([lm,'tee','read',key],env=env)==raw.encode()
from headroom.transforms.cross_turn_dedup import DedupBlock,dedup_blocks
blocks,stats=dedup_blocks([DedupBlock(log,0),DedupBlock(log,1)])
hr=json.dumps([dict(role='tool',content=b.text) for b in blocks],separators=(',',':'),ensure_ascii=False)
reference=subprocess.run([rtk,'proxy',*producer],input=raw,text=True,capture_output=True,env=env,check=True).stdout
assert reference==raw
conversation=dict(source='controlled repeat of ripgrep-log, two tool messages',raw_tokens=count(raw),rtk_tokens=count(reference),lm_tokens=count(view),headroom_specialized_tokens=count(hr),exact_message_inverse=True,tee_exact=True,headroom_stats=stats)
result=dict(binary_sha256=hashlib.sha256(lm.read_bytes()).hexdigest(),headroom_version=__import__('importlib.metadata',fromlist=['version']).version('headroom-ai'),tokenizer='o200k_base',source='unchanged real captures plus explicitly controlled Windows shapes, unregistered Python producer',cases=rows,conversation=conversation)
(work/'results.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
