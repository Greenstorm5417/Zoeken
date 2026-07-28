#!/usr/bin/env bash
# Low-power profiling harness: Criterion + flamegraph + cargo-bloat.
# Requires cargo-flamegraph / perf / cargo-bloat on PATH when those steps run.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

OUT="${PERF_OUT:-$ROOT/target/perf}"
mkdir -p "$OUT"

echo "==> criterion (zoeken-search aggregation)"
cargo bench -p zoeken-search --bench aggregation -- --output-format bencher | tee "$OUT/criterion-bencher.txt"

echo "==> cargo bloat (zoeken-server release)"
cargo bloat --release --locked --bin zoeken-server -n 40 | tee "$OUT/bloat-functions.txt"
cargo bloat --release --locked --bin zoeken-server --crates -n 40 | tee "$OUT/bloat-crates.txt"
cargo bloat --release --locked --bin zoeken-server --crates --wide | tee "$OUT/bloat-crates-wide.txt"

echo "==> binary size"
BIN="$(cargo metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p' | head -1)/release/zoeken-server"
if [[ -x "$BIN" ]]; then
  ls -lh "$BIN" | tee "$OUT/binary-size.txt"
  if command -v llvm-strip >/dev/null 2>&1 || command -v strip >/dev/null 2>&1; then
    cp -f "$BIN" "$OUT/zoeken-server.unstripped"
    ls -lh "$OUT/zoeken-server.unstripped" >>"$OUT/binary-size.txt"
  fi
fi

if command -v flamegraph >/dev/null 2>&1 || command -v cargo-flamegraph >/dev/null 2>&1; then
  echo "==> flamegraph (aggregation bench)"
  # Needs perf; may require: sudo sysctl kernel.perf_event_paranoid=1
  CARGO_PROFILE_BENCH_DEBUG=true \
    cargo flamegraph \
      -p zoeken-search \
      --bench aggregation \
      -o "$OUT/aggregation-flamegraph.svg" \
      -- || {
        echo "flamegraph failed (often perf permissions); criterion+bloat still saved under $OUT" >&2
      }
else
  echo "skip flamegraph: cargo-flamegraph not installed" >&2
fi

echo "artifacts in $OUT"
