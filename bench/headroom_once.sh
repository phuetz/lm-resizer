#!/bin/sh
# Adapter for Headroom's public Python API. The interpreter lives in target/banc.
exec "$1" -c '
import sys
from headroom import compress
content = sys.stdin.read()
messages = [
    {"role": "user", "content": "Diagnose this tool result using its exact facts."},
    {"role": "assistant", "content": None, "tool_calls": [
        {"id": "call_banc", "type": "function", "function": {"name": "shell", "arguments": "{}"}}
    ]},
    {"role": "tool", "tool_call_id": "call_banc", "content": content},
]
result = compress(messages, model="gpt-4o")
output = result.messages[-1]["content"]
if not isinstance(output, str):
    raise TypeError("Headroom returned non-text content")
sys.stdout.write(output)
'
