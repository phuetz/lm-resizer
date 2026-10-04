#!/usr/bin/env python3
"""Foreground-only Headroom probes on real installed metadata/source and a controlled log workload."""
import importlib.metadata
import json
import logging
from pathlib import Path
import subprocess
import sys
import time
import tiktoken

root = Path(__file__).resolve().parents[2]
work = root / 'target/reprise-1/headroom-probe'
work.mkdir(parents=True, exist_ok=True)
# Metadata returned by the installed Python environment, no personal filesystem fields.
metadata = sorted([dict(name=d.metadata['Name'], version=d.version,
                       license=d.metadata.get('License-Expression') or 'unspecified')
                   for d in importlib.metadata.distributions()], key=lambda d: d['name'])
logs = subprocess.check_output([sys.executable, '-c', '''
import logging
logging.basicConfig(format="%(levelname)s %(message)s", level=logging.INFO)
for i in range(500):
    logging.info("GET /health 200 service=api")
logging.error("request_failed request_id=42 reason=database_unavailable")
for i in range(500):
    logging.info("GET /health 200 service=api")
'''], stderr=subprocess.STDOUT).decode()
cases = {'python-distributions': json.dumps(metadata, indent=2),
         'health-log': logs,
         'rust-source': (root/'src/lossless_filters.rs').read_text()}
enc = tiktoken.get_encoding('o200k_base')
count = lambda s: len(enc.encode(s, disallowed_special=()))
rows=[]
for name, raw in cases.items():
    (work/(name+'.raw')).write_text(raw)
    start = time.perf_counter()
    result = subprocess.run(['sh', str(root/'bench/headroom_once.sh'), sys.executable],
                            input=raw, capture_output=True, text=True, timeout=120)
    ms=(time.perf_counter()-start)*1000
    (work/(name+'.headroom')).write_text(result.stdout)
    row=dict(case=name, raw_tokens=count(raw), headroom_tokens=count(result.stdout),
             ms=ms, exit_code=result.returncode, error=result.stderr[-1000:])
    if name == 'health-log':
        from headroom.transforms.log_compressor import LogCompressor
        start=time.perf_counter()
        out=LogCompressor().compress(raw).compressed
        (work/(name+'.log')).write_text(out)
        row.update(log_tokens=count(out), log_ms=(time.perf_counter()-start)*1000)
    if name == 'rust-source':
        from headroom.transforms.code_compressor import compress_code
        start=time.perf_counter()
        out=compress_code(raw, language='rust')
        (work/(name+'.ast')).write_text(out)
        row.update(ast_tokens=count(out), ast_ms=(time.perf_counter()-start)*1000)
    rows.append(row)
(work/'results.json').write_text(json.dumps(rows, indent=2)+'\n')
print(json.dumps(rows, indent=2))
