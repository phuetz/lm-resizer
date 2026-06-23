#!/usr/bin/env sh
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
binary="$root/target/release/lm-resizer"
if [ ! -x "$binary" ]; then
  printf >&2 '%s\n' "missing release binary: $binary"
  exit 1
fi

port="${LM_RESIZER_SMOKE_PORT:-18787}"
bind="127.0.0.1:$port"
"$binary" serve --bind "$bind" >/tmp/lm-resizer-smoke-proxy.log 2>&1 &
pid="$!"

cleanup() {
  kill "$pid" >/dev/null 2>&1 || true
  wait "$pid" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

i=0
while [ "$i" -lt 70 ]; do
  if curl -fsS "http://$bind/health" >/dev/null 2>&1; then
    break
  fi
  i=$((i + 1))
  sleep 0.15
done

if [ "$i" -ge 70 ]; then
  printf >&2 '%s\n' "proxy did not become healthy at http://$bind/health"
  cat /tmp/lm-resizer-smoke-proxy.log >&2 || true
  exit 1
fi

response="$(curl -fsS \
  -H 'content-type: application/json' \
  -d '{"model":"gpt-test","messages":[{"role":"user","content":"Summarize this repeated build output: error: compile failed\nerror: compile failed\nerror: compile failed"}]}' \
  "http://$bind/v1/chat/completions")"

printf '%s' "$response" | grep -q '"mode":"preview"'
printf '%s' "$response" | grep -q '"compression"'
printf '%s' "$response" | grep -q '"request"'
printf '%s\n' "proxy preview smoke passed at http://$bind"
