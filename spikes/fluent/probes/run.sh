#!/bin/sh
# Compiles every probe against the built spike and prints the compiler's verdict for each.
# Expected: a5_outermost_fnptr.rs and ok_variants.rs compile, every other probe fails.
cd "$(dirname "$0")/.." && cargo build -q && mkdir -p target/probes || exit 1
for f in probes/*.rs probes/shape_a/*.rs; do
    [ "${f##*/}" = model.rs ] && continue
    echo "=== $f"
    rustc --edition 2024 --crate-type lib --emit=metadata --out-dir target/probes \
        --extern drawnui_spike=target/debug/libdrawnui_spike.rlib "$f" 2>&1
    echo "exit=$?"
done
