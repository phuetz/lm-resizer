#!/usr/bin/env python3
"""Regression: chronological tee and RTK's stdout-then-stderr fallback view."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
root=Path(__file__).resolve().parents[2]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--work', type=Path, required=True)
args=parser.parse_args()
work=args.work.resolve()
work.mkdir(parents=True,exist_ok=False)
shim=work/'pytest'
shim.write_text('#!/usr/bin/env python3\nimport os\nfor i in range(8):\n os.write(1, f"OUT{i:04d}\\n".encode())\n os.write(2, f"ERR{i:04d}\\n".encode())\nraise SystemExit(3)\n')
shim.chmod(0o755)
env=dict(os.environ,PATH=str(work)+os.pathsep+os.environ['PATH'],
    LM_RESIZER_STATE_DIR=str(work/'state'),LM_RESIZER_STORE=str(work/'ccr.sqlite'),
    XDG_DATA_HOME=str(work/'data'),XDG_CONFIG_HOME=str(work/'config'),
    RTK_TELEMETRY_DISABLED='1',RTK_SUPPRESS_HOOK_WARNING='1')
lm=root/'target/release/lm-resizer'
rtk=root/'target/rtk-parity/bin/rtk'
pin=json.loads((root/'bench/rtk-parity/reference.json').read_text())
assert hashlib.sha256(rtk.read_bytes()).hexdigest()==pin['binary_sha256']
ref=subprocess.run([rtk,'pytest'],capture_output=True,env=env)
p=subprocess.run([lm,'exec','--json','--','pytest'],capture_output=True,env=env)
report=json.loads(p.stdout)
expected=''.join(f'OUT{i:04d}\nERR{i:04d}\n' for i in range(8)).encode()
key=report['tee_hint'].removeprefix('[raw: ').removesuffix(']') if report['tee_hint'] else None
if key:
    recovered=subprocess.check_output([lm,'tee','read',key],env=env)
else:
    recovered=report['output'].encode()
assert recovered==expected
assert p.returncode==ref.returncode==3
body=report['output'].encode()[:report['filtered_bytes']]
assert body==ref.stdout+ref.stderr, 'mixed-stream view differs from RTK'
result=dict(command='pytest (alternating shim)',expected=expected.decode(),
    reference=(ref.stdout+ref.stderr).decode(),lm_view=body.decode(),
    strict_parity=body==ref.stdout+ref.stderr,raw_exact=True,
    recovery='tee' if key else 'unchanged view',exit_code=3,
    lm_sha256=hashlib.sha256(lm.read_bytes()).hexdigest())
(root/'bench/rtk-parity/mixed-streams.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
