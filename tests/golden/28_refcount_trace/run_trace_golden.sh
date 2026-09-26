#!/bin/bash
# run_trace_golden.sh — run the refcount tracer (-trace-refcount) over
# tests/golden/28_refcount_trace/*.np and diff the output against *.out.
# Usage: ./run_trace_golden.sh
#   NPAC    path to the nupac binary. Auto-detected: prefer an explicit $NPAC,
#           then target/release/nupac, then target/debug/nupac. Auto-detection
#           matters because a stale binary silently produces line-number-only
#           diffs that look like real regressions.
set -u
cd "$(dirname "$0")/../../.."

if [[ -z "${NPAC:-}" ]]; then
    for cand in target/release/nupac target/debug/nupac; do
        if [[ -x "$cand" ]]; then NPAC="$cand"; break; fi
    done
fi
if [[ -z "${NPAC:-}" || ! -x "$NPAC" ]]; then
    echo "error: nupac binary not found (build it, or set NPAC=)" >&2
    exit 2
fi
echo "using nupac: $NPAC"
DIR=tests/golden/28_refcount_trace
PASS=0
FAIL=0

for np in "$DIR"/*.np; do
    out="${np%.np}.out"
    if [[ ! -f "$out" ]]; then
        echo "SKIP  $np (no $out)"
        continue
    fi
    # trace with colors disabled for deterministic diffing
    tmp=/tmp/refcount_trace.$$.txt
    "$NPAC" -trace-refcount -trace-no-color -trace-max-iters 2 "$np" > "$tmp" 2>&1
    rc=$?
    if [[ $rc -ne 0 ]]; then
        echo "FAIL  $np (tracer exit $rc)"
        sed 's/^/      /' "$tmp"
        rm -f "$tmp"
        FAIL=$((FAIL + 1))
        continue
    fi
    if diff -u "$out" "$tmp" > /tmp/refcount_trace.diff 2>&1; then
        rm -f "$tmp"
        echo "PASS  $np"
        PASS=$((PASS + 1))
    else
        echo "FAIL  $np"
        sed 's/^/      /' /tmp/refcount_trace.diff
        rm -f "$tmp"
        FAIL=$((FAIL + 1))
    fi
done

echo "----"
echo "trace golden: $PASS passed, $FAIL failed"
exit $((FAIL > 0))
