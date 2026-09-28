#!/usr/bin/env bash
# One-command, isolated replay. Run from any checkout; pass a report path if desired.
set -euo pipefail
repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
qa_dir="$repo_dir/_qa/banc"
mkdir -p "$qa_dir"
cd "$repo_dir"

if [[ ! -x "$qa_dir/rtk/rtk" ]]; then
  curl -fsSL -o "$qa_dir/rtk.tar.gz" https://github.com/rtk-ai/rtk/releases/download/v0.50.0/rtk-x86_64-unknown-linux-musl.tar.gz
  printf '%s  %s\n' 'bc2b8902b0d9c796c82ef45f16ae2307e17757afeca5ee156235a3dc7bda5f89' "$qa_dir/rtk.tar.gz" | sha256sum -c -
  mkdir -p "$qa_dir/rtk"
  tar -xzf "$qa_dir/rtk.tar.gz" -C "$qa_dir/rtk"
fi
if [[ ! -x "$qa_dir/venv/bin/python" ]]; then
  python3 -m venv "$qa_dir/venv"
fi
if ! "$qa_dir/venv/bin/python" -c 'import importlib.metadata as m; assert m.version("headroom-ai") == "0.39.1"; assert m.version("tiktoken") == "0.14.0"; assert m.version("pytest") == "9.1.1"; assert m.version("onnxruntime") == "1.24.4"' >/dev/null 2>&1; then
  if command -v uv >/dev/null 2>&1; then
    UV_CACHE_DIR="$qa_dir/uv-cache" uv pip install --python "$qa_dir/venv/bin/python" 'headroom-ai[code]==0.39.1' 'tiktoken==0.14.0' 'pytest==9.1.1' 'onnxruntime==1.24.4' >"$qa_dir/install.log" 2>&1
  else
    PIP_CACHE_DIR="$qa_dir/pip-cache" "$qa_dir/venv/bin/python" -m pip install 'headroom-ai[code]==0.39.1' 'tiktoken==0.14.0' 'pytest==9.1.1' 'onnxruntime==1.24.4' >"$qa_dir/install.log" 2>&1
  fi
fi
python3 bench/build_corpus.py
"$qa_dir/venv/bin/python" -m unittest discover -s bench -p 'test_banc.py' >"$qa_dir/tests.log" 2>&1
if [[ -n "${LM_RESIZER_BIN:-}" ]]; then
  lm_bin="$LM_RESIZER_BIN"
else
  CARGO_HOME="$qa_dir/cargo-home" CARGO_TARGET_DIR="$qa_dir/lm-target" cargo build --release --bin lm-resizer >"$qa_dir/build.log" 2>&1
  lm_bin="$qa_dir/lm-target/release/lm-resizer"
fi
report_path="${1:-$qa_dir/RAPPORT.md}"
"$qa_dir/venv/bin/python" bench/run.py --lm-bin "$lm_bin" --report "$report_path" | tee "$qa_dir/run.log"
if [[ "${BANC_SKIP_AGENT:-0}" != "1" ]] && command -v codex >/dev/null 2>&1 && [[ -r "$HOME/.codex/auth.json" ]]; then
  "$qa_dir/venv/bin/python" bench/agent_eval.py | tee "$qa_dir/agent-run.log"
  "$qa_dir/venv/bin/python" bench/run.py --report-only --lm-bin "$lm_bin" --report "$report_path"
fi
