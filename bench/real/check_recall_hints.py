#!/usr/bin/env python3
"""Independent reference for RTK's second recall form, with working LM recovery."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tiktoken

root=Path(__file__).resolve().parents[2]
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--work', type=Path, required=True)
a=p.parse_args(); work=a.work.resolve(); work.mkdir(parents=True,exist_ok=False)
lm=root/'target/release/lm-resizer'; rtk=root/'target/rtk-parity/bin/rtk'
assert hashlib.sha256(rtk.read_bytes()).hexdigest()==json.loads((root/'bench/rtk-parity/reference.json').read_text())['binary_sha256']
raw='Test project /tmp/build\n\n0% tests passed, 25 tests failed out of 25\n\nThe following tests FAILED:\n'+''.join(f'\t  {n} - broken_{n} (Failed)\n' for n in range(1,26))
(work/'raw').write_text(raw)
shim=work/'ctest'; shim.write_text('#!/bin/sh\n/bin/cat "$RECALL_CAPTURE"\nexit 8\n');shim.chmod(0o755)
env=dict(os.environ,PATH=str(work)+os.pathsep+os.environ['PATH'],RECALL_CAPTURE=str(work/'raw'),
         LM_RESIZER_TRACKING='0', RTK_TELEMETRY_DISABLED='1',RTK_SUPPRESS_HOOK_WARNING='1',
         LM_RESIZER_STATE_DIR=str(work/'state'), LM_RESIZER_STORE=str(work/'ccr.sqlite'),
         XDG_CONFIG_HOME=str(work/'config'),XDG_DATA_HOME=str(work/'data'))
ref=subprocess.run([rtk,'ctest'],env=env,capture_output=True)
res=subprocess.run([lm,'exec','--json','--','ctest'],env=env,capture_output=True)
report=json.loads(res.stdout)
view=report['output'].encode()[:report['filtered_bytes']].decode(); reference=(ref.stdout+ref.stderr).decode()
commands=re.findall(r'lm-resizer tee read ([0-9a-f]{12})',view)
assert commands and 'rtk recall' not in view
for key in commands:
    read=subprocess.run([lm,'tee','read',key],env=env,capture_output=True)
    assert read.returncode==0 and read.stdout==raw.encode()
expected=re.sub(r'\[full output: rtk recall [0-9a-f]{12}\]',f'[lm-resizer tee read {commands[0]}]',reference)
expected=re.sub(r'rtk recall [0-9a-f]{12}',f'lm-resizer tee read {commands[0]}',expected)
(work/'reference').write_text(reference)
(work/'view').write_text(view)
assert view==expected, (reference, view)
assert ref.returncode==res.returncode==8
enc=tiktoken.get_encoding('o200k_base'); count=lambda s:len(enc.encode(s,disallowed_special=()))
result=dict(case='ctest: 25 reported failures emitted by a shim, hidden-tail recall',raw_sha256=hashlib.sha256(raw.encode()).hexdigest(),
            lm_sha256=hashlib.sha256(lm.read_bytes()).hexdigest(),reference=reference,lm_view=view,
            rtk_tokens=count(reference),lm_view_tokens=count(view),lm_total_tokens=count(report['output']),
            view_token_delta=count(view)-count(reference),authorized_recall_adaptation=True,tee_verified=True,
            producer_exit=8,reference_exit=ref.returncode,lm_exit=res.returncode)
(work/'results.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
