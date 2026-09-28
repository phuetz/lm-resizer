#!/usr/bin/env python3
"""One isolated Headroom call; stdout contains only the resulting tool content."""
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
    raise TypeError(f"Unexpected compressed content: {type(output)!r}")
sys.stdout.write(output)
