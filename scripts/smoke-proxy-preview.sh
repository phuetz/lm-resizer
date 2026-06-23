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

# Real compression assertion: a noisy `tool` message (large uniform JSON array)
# must be compressed by the provider-aware live-zone path (bytes_saved > 0),
# not merely echoed back. This exercises the actual /v1 compression seam over
# HTTP, not just the envelope shape.
noisy_req="$(node -e '
  const rows = Array.from({length: 60}, (_, i) => ({id: i, name: "item-" + i, status: "ok", score: 100}));
  process.stdout.write(JSON.stringify({
    model: "gpt-4o",
    messages: [
      {role: "user", content: "summarize the results"},
      {role: "assistant", content: "calling tool"},
      {role: "tool", tool_call_id: "t1", content: JSON.stringify(rows)},
    ],
  }));
')"
noisy_resp="$(curl -fsS -H 'content-type: application/json' -d "$noisy_req" "http://$bind/v1/chat/completions")"
printf '%s' "$noisy_resp" | node -e '
  const d = JSON.parse(require("node:fs").readFileSync(0, "utf8"));
  const saved = d.compression && d.compression.bytes_saved;
  if (!(saved > 0)) {
    console.error("expected provider-aware live-zone compression, bytes_saved=" + saved);
    process.exit(1);
  }
  console.error("live-zone compression saved " + saved + " bytes over HTTP");
'

printf '%s\n' "proxy preview smoke passed at http://$bind"
