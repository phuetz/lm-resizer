#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_dir"
qa_dir="$repo_dir/target/rtk-parity"
mkdir -p "$qa_dir"
export UV_CACHE_DIR="$qa_dir/uv-cache"
export CARGO_HOME="${CARGO_HOME:-$qa_dir/cargo-home}"
curl -fsSL https://raw.githubusercontent.com/rtk-ai/rtk/1d87b8e719ce0a50c223cd93ca64dd16921f9aec/install.sh -o "$qa_dir/install.sh"
python3 - "$qa_dir/install.sh" <<'PY'
import hashlib, json, sys
from pathlib import Path
pinned = json.loads(Path('bench/rtk-parity/reference.json').read_text())
assert hashlib.sha256(Path(sys.argv[1]).read_bytes()).hexdigest() == pinned['installer_sha256']
PY
# The official script verifies the release archive against checksums.txt.
RTK_VERSION=v0.50.0 RTK_INSTALL_DIR="$qa_dir/bin" RTK_SKIP_CHECKSUM=0 sh "$qa_dir/install.sh"
if [[ ! -x "$qa_dir/venv/bin/python" ]]; then
  uv venv "$qa_dir/venv"
fi
uv pip install --python "$qa_dir/venv/bin/python" 'tiktoken==0.14.0' 'headroom-ai[code]==0.39.1' 'onnxruntime==1.24.4'
cargo build --release
# A fresh output directory is mandatory: prior measurements are never overwritten.
report_dir="${1:-$qa_dir/replay-$(date +%Y%m%d-%H%M%S)}"
"$qa_dir/venv/bin/python" bench/real/parity_rtk.py \
  --binary target/release/lm-resizer --rtk "$qa_dir/bin/rtk" \
  --headroom-python "$qa_dir/venv/bin/python" --work "$report_dir"
