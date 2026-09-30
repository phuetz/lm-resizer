#!/usr/bin/env bash
set -euo pipefail

if [ -z "${LM_RESIZER_BIN:-}" ]; then
    cargo build --release
    LM_RESIZER_BIN=target/release/lm-resizer
fi

# Keep the CCR store and command history isolated from the user profile.
STATE_DIR=$(mktemp -d)
trap 'rm -rf "$STATE_DIR"' EXIT
export LM_RESIZER_STATE_DIR="$STATE_DIR"

CASES=$(jq -r '.[].file' bench/cases.json)

run_tests() {
    local route="$1"
    local exit_code=0
    for case_file in $CASES; do
        if [ ! -f "bench/$case_file" ]; then
            echo "Missing fixture: bench/$case_file" >&2
            return 1
        fi
        command=$(jq -r --arg file "$case_file" '.[] | select(.file == $file) | .command' bench/cases.json)

        echo "Testing $case_file via $route (command: $command)..."
        DIR1=$(mktemp -d)
        DIR2=$(mktemp -d)

        if [ "$route" = "exec" ]; then
            # exec runs cat on the fixture; this checks the exec pipeline,
            # not the fixture command named in cases.json.
            cat "bench/$case_file" | "$LM_RESIZER_BIN" exec -- cat > "$DIR1/out.txt"
            cat "bench/$case_file" | "$LM_RESIZER_BIN" exec -- cat > "$DIR2/out.txt"
        elif [ "$route" = "tool-output" ]; then
            cat "bench/$case_file" | "$LM_RESIZER_BIN" tool-output --command "$command" > "$DIR1/out.txt"
            cat "bench/$case_file" | "$LM_RESIZER_BIN" tool-output --command "$command" > "$DIR2/out.txt"
        elif [ "$route" = "compress" ]; then
            cat "bench/$case_file" | "$LM_RESIZER_BIN" compress > "$DIR1/out.txt"
            cat "bench/$case_file" | "$LM_RESIZER_BIN" compress > "$DIR2/out.txt"
        fi

        if ! cmp -s "$DIR1/out.txt" "$DIR2/out.txt"; then
            echo "ERROR: Non-deterministic output for $case_file via $route"
            diff -u "$DIR1/out.txt" "$DIR2/out.txt"
            exit_code=1
        fi

        rm -rf "$DIR1" "$DIR2"
    done
    return $exit_code
}

run_tests "tool-output"
run_tests "compress"
run_tests "exec"

echo "All tests passed!"
