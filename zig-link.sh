#!/bin/bash
# zig-link.sh — 把 rustc 的链接参数传给 zig cc（用于跨平台链接）
# 过滤掉 -nostartfiles（zig cc 和 rustc 的 startup 冲突）
set -euo pipefail
ARGS=()
for a in "$@"; do
    case "$a" in
        -nostartfiles) ;;
        *) ARGS+=("$a") ;;
    esac
done
exec zig cc "${ARGS[@]}"