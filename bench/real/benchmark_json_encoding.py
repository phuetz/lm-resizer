#!/usr/bin/env python3
"""Compare typed JSON cells with CSV-escaped JSON cells, without changing runtime."""
import csv,io,json
import tiktoken
enc=tiktoken.get_encoding('o200k_base')
items=[dict(id=n,name=f'invoice-{n}',status='pending',note='one, "quoted" note' if n==1 else 'fine',value=None if n==0 else dict(amount=n)) for n in range(20)]
columns=sorted(items[0]); rows=[[r[c] for c in columns] for r in items]
compact=lambda x:json.dumps(x,separators=(',',':'),ensure_ascii=False)
table='JSON table (all 20 rows; cells follow columns):\n'+compact(dict(columns=columns,rows=rows))+'\n'
s=io.StringIO();writer=csv.writer(s,lineterminator='\n');writer.writerow(columns)
for row in rows:writer.writerow([compact(v) for v in row])
candidate='JSON cells as CSV:\n'+s.getvalue()
reader=csv.reader(io.StringIO(s.getvalue()));keys=next(reader)
assert [dict(zip(keys,map(json.loads,row))) for row in reader]==items
count=lambda x:len(enc.encode(x,disallowed_special=()))
print(json.dumps(dict(source='controlled mixed values, 20 rows',raw_tokens=count(compact(items)),typed_table_tokens=count(table),csv_json_cells_tokens=count(candidate),csv_exact=True,decision='keep typed table; CSV candidate is larger'),indent=2))
