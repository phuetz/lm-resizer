#!/usr/bin/env python3
"""Controlled syntax-index outline probes; no claim of real-session frequency."""
import argparse,hashlib,json,os,subprocess
from pathlib import Path
import tiktoken
from headroom.transforms.code_compressor import compress_code
p=argparse.ArgumentParser();p.add_argument('--work',type=Path,required=True);a=p.parse_args()
root=Path(__file__).resolve().parents[2];work=a.work.resolve();work.mkdir(parents=True,exist_ok=False)
project=work/'project';project.mkdir();(work/'home').mkdir()
env=dict(os.environ,HOME=str(work/'home'),LM_RESIZER_STATE_DIR=str(work/'state'),LM_RESIZER_STORE=str(work/'ccr.sqlite'),RTK_TELEMETRY_DISABLED='1',RTK_SUPPRESS_HOOK_WARNING='1',XDG_DATA_HOME=str(work/'data'),XDG_CONFIG_HOME=str(work/'config'))
sources={
'invoice.py':'BRACES = "literal { value }"\n\ndef short(): return 1\n\ndef invoice(value: int) -> int:\n    """Keep this documentation."""\n'+''.join(f'    value += {n}\n' for n in range(40))+'    return value\n',
'invoice.ts':'export const braces = "literal { value }";\nexport function short() { return 1; }\nexport function invoice(value: number): number {\n'+''.join(f'    value += {n};\n' for n in range(40))+'    return value;\n}\n',
'invalid.py':'def broken(:\n  return 1\n'}
for name,raw in sources.items():(project/name).write_text(raw)
index=subprocess.run(['code-explorer','analyze',str(project),'--skip-git','--no-docs','--max-files','5'],env=env,capture_output=True,text=True,check=True)
(work/'index.log').write_text(index.stdout+index.stderr)
lm=root/'target/release/lm-resizer';rtk=root/'target/rtk-parity/oracle-build/release/rtk'
receipt=json.loads((root/'target/rtk-parity/oracle-build/receipt.json').read_text());assert hashlib.sha256(rtk.read_bytes()).hexdigest()==receipt['binary_sha256']
enc=tiktoken.get_encoding('o200k_base');count=lambda s:len(enc.encode(s,disallowed_special=()))
rows=[]
for name,raw in sources.items():
    def call(args):return subprocess.check_output(args,env=env,text=True)
    view=call([lm,'outline',project/name]);reference=call([rtk,'read',project/name]);hr=compress_code(raw,language='python' if name.endswith('.py') else 'typescript')
    assert call([lm,'tee','read',hashlib.sha256(raw.encode()).hexdigest()])==raw
    cat=json.loads(call([lm,'exec','--json','--','cat',project/name]));assert cat['output']==raw
    if name=='invalid.py':assert view==raw
    else:
        assert 'literal { value }' in view and 'short()' in view
        assert ('def invoice(value: int) -> int:' if name.endswith('.py') else 'export function invoice(value: number): number {') in view
        assert count(view)<count(raw)
    rows.append(dict(id=name,raw_tokens=count(raw),lm_total_tokens=count(view),rtk_read_tokens=count(reference),headroom_code_tokens=count(hr),tee_exact=True,cat_unchanged=True,invalid_unchanged=name=='invalid.py',source='controlled syntax fixture, indexed locally'))
result=dict(binary_sha256=hashlib.sha256(lm.read_bytes()).hexdigest(),code_explorer_version=call(['code-explorer','--version']).strip(),cases=rows)
(work/'results.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
