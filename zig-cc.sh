#!/bin/bash
# zig-cc.sh — 把 cc-rs 的 --target=<rust-triple> 翻译成 zig 的 target 格式
set -euo pipefail

# 提取 --target=<triple>
ARGS=()
ZIG_TARGET=""
for a in "$@"; do
    case "$a" in
        --target=*)
            t="${a#--target=}"
            case "$t" in
                x86_64-unknown-linux-musl)  ZIG_TARGET="x86_64-linux-musl" ;;
                aarch64-unknown-linux-musl) ZIG_TARGET="aarch64-linux-musl" ;;
                x86_64-pc-windows-gnu)      ZIG_TARGET="x86_64-windows-gnu" ;;
                aarch64-apple-darwin)       ZIG_TARGET="aarch64-macos" ;;
                x86_64-apple-darwin)        ZIG_TARGET="x86_64-macos" ;;
                *) ZIG_TARGET="$t" ;;
            esac
            ;;
        *) ARGS+=("$a") ;;
    esac
done

if [ -n "$ZIG_TARGET" ]; then
    exec zig cc -target "$ZIG_TARGET" "${ARGS[@]}"
else
    exec zig cc "${ARGS[@]}"
fi