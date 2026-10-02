#!/usr/bin/env python3
"""Child used by exec: replay captured streams without changing command routing."""
import json
import os
from pathlib import Path

capture = Path(os.environ["LMR_CAPTURE"])
os.write(1, (capture / "stdout").read_bytes())
os.write(2, (capture / "stderr").read_bytes())
raise SystemExit(json.loads((capture / "meta.json").read_text())["exit_code"])
