#!/bin/bash
# examples/03_LibUI/run_libui.sh — transpile, compile, link, run the libui-ng demo
# Requires: libui-ng (set LIBUI_DIR to point to your checkout)
#           nupac (built at ../../target/debug/nupac)
set -e

NUPAC="${NUPAC:-../../target/debug/nupac}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NUPALANG="$(cd "$SCRIPT_DIR/../.." && pwd)"
if [ -n "${LIBUI_DIR:-}" ]; then
    LIBUI="$LIBUI_DIR"
elif [ -d "$NUPALANG/../libui-ng" ]; then
    LIBUI="$(cd "$NUPALANG/../libui-ng" && pwd)"
else
    echo "Error: libui-ng not found. Set LIBUI_DIR to your libui-ng checkout." >&2
    exit 1
fi

echo "==> Transpile..."
mkdir -p /tmp/libui_build
"$NUPAC" -rewrite-nupa "$SCRIPT_DIR/libui_demo.np" -o /tmp/libui_build/libui_demo.c -fno-nupa-arc -I "$SCRIPT_DIR/include" -I "$LIBUI"
"$NUPAC" -rewrite-nupa "$SCRIPT_DIR/include/LibUI.np" -o /tmp/libui_build/LibUI.c -fno-nupa-arc -I "$SCRIPT_DIR/include" -I "$LIBUI"

echo "==> Compile + Link..."
clang -std=c99 -fblocks \
    -I "$SCRIPT_DIR/include" \
    -I "$NUPALANG/include" \
    -I "$NUPALANG/include/Foundation" \
    -I "$NUPALANG" \
    -I "$LIBUI" \
    -x c /tmp/libui_build/libui_demo.c /tmp/libui_build/LibUI.c \
    "$NUPALANG/include/nupa/runtime.c" \
    -x objective-c "$SCRIPT_DIR/include/LibUI_mac.c" \
    -L "$LIBUI/build/meson-out" -lui \
    -Wl,-rpath,"$LIBUI/build/meson-out" \
    -framework Cocoa \
    -o /tmp/libui_build/libui_demo -w

echo "==> Run..."
/tmp/libui_build/libui_demo