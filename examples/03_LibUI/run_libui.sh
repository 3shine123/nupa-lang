#!/bin/bash
# examples/03_LibUI/run_libui.sh — transpile, compile, link, run the libui-ng demo.
#
# Pure Nupa: the only sources are .np/.nh files. The demo inlines the wrapper
# (include/LibUI.np → one .c file), which is compiled and linked with the
# Nupa runtime — no hand-written .c/.m files anywhere.
#
# Requires: libui-ng built with meson (set LIBUI_DIR to your checkout)
#           nupac (built at ../../target/debug/nupac or ../../target/release/nupac)
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NUPALANG="$(cd "$SCRIPT_DIR/../.." && pwd)"

if [ -n "${NUPAC:-}" ]; then
    NUPAC="$NUPAC"
elif [ -x "$NUPALANG/target/debug/nupac" ]; then
    NUPAC="$NUPALANG/target/debug/nupac"
else
    NUPAC="$NUPALANG/target/release/nupac"
fi

if [ -n "${LIBUI_DIR:-}" ]; then
    LIBUI="$LIBUI_DIR"
elif [ -d "$NUPALANG/../libui-ng" ]; then
    LIBUI="$(cd "$NUPALANG/../libui-ng" && pwd)"
else
    echo "Error: libui-ng not found. Set LIBUI_DIR to your libui-ng checkout." >&2
    exit 1
fi

BUILD=/tmp/libui_build
rm -rf "$BUILD"
mkdir -p "$BUILD"

echo "==> Transpile (nupac)..."
"$NUPAC" -rewrite-nupa "$SCRIPT_DIR/libui_demo.np" -o "$BUILD/libui_demo.c" \
    -I "$SCRIPT_DIR/include" -I "$LIBUI"

echo "==> Compile + link (clang)..."
FLAGS="-std=c99 -fblocks -w"
INCLUDES=(-I "$NUPALANG/include" -I "$NUPALANG/include/Foundation" -I "$LIBUI")
if [ "$(uname)" = "Darwin" ]; then
    FRAMEWORKS=(-framework Cocoa)
else
    FRAMEWORKS=()
fi

clang $FLAGS "${INCLUDES[@]}" \
    -x c "$BUILD/libui_demo.c" \
    "$NUPALANG/include/nupa/runtime.c" \
    -L "$LIBUI/build/meson-out" -lui \
    -Wl,-rpath,"$LIBUI/build/meson-out" \
    "${FRAMEWORKS[@]}" \
    -o "$BUILD/libui_demo"

echo "==> Run..."
"$BUILD/libui_demo"
