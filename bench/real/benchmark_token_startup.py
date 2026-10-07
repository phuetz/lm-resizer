#!/usr/bin/env python3
"""Interleaved fresh-process profiling; exact counts checked independently."""
import argparse,hashlib,json,os,re,statistics,subprocess,time
from pathlib import Path
import tiktoken
p=argparse.ArgumentParser();p.add_argument('--before',type=Path,required=True);p.add_argument('--after',type=Path,required=True);p.add_argument('--work',type=Path,required=True);a=p.parse_args()
w=a.work.resolve();w.mkdir(parents=True,exist_ok=False);(w/'home').mkdir()
bins={k:getattr(a,k).resolve() for k in ('before','after')}
env=dict(os.environ,HOME=str(w/'home'),LM_RESIZER_STATE_DIR=str(w/'state'),LM_RESIZER_STORE=str(w/'store.sqlite'),LM_RESIZER_TRACKING='0',LM_RESIZER_PROFILE='1')
enc=tiktoken.get_encoding('o200k_base');rows=[]
for name,raw in [('ascii',"invoice status unknown 123456789 isn't done\n"),('unicode','état inconnu 漢字 العربية 🦀\n')]:
    samples={k:[] for k in bins};profiles={k:[] for k in bins};counts={k:[] for k in bins}
    for repeat in range(9):
        for key in (list(bins) if repeat%2 else list(reversed(bins))):
            start=time.perf_counter();r=subprocess.run([bins[key],'pipe','--filter','pytest','--json'],input=raw,text=True,env=env,capture_output=True,check=True)
            samples[key].append((time.perf_counter()-start)*1000)
            report=json.loads(r.stdout);counts[key].append((report['original_tokens'],report['compressed_tokens']))
            assert report['original_tokens']==len(enc.encode(raw,disallowed_special=()))
            assert report['compressed_tokens']==len(enc.encode(report['output'],disallowed_special=()))
            profiles[key].append(r.stderr)
    assert counts['before']==counts['after']
    stage=lambda k:statistics.median(float(re.search(r'profile original_token_count: ([\d.]+)s',s)[1])*1000 for s in profiles[k])
    rows.append(dict(input=name,samples_ms=samples,median_ms={k:statistics.median(v) for k,v in samples.items()},first_count_median_ms={k:stage(k) for k in bins},profiles=profiles,exact_counts_equal=True))
result=dict(sha256={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in bins.items()},repetitions=9,cases=rows)
(w/'results.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps([{k:v for k,v in row.items() if k not in ('profiles','samples_ms')} for row in rows],indent=2))
