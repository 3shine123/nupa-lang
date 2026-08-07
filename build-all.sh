#!/usr/bin/env bash
# build-all.sh — 一键交叉编译 nupac 到 5 个目标平台
# 依赖：zig 0.14+（brew install zig），mingw-w64（brew install mingw-w64），
#       rustup 管理的 Rust
set -euo pipefail
cd "$(dirname "$0")"

source "$HOME/.cargo/env"
ZIG_CC="$(pwd)/zig-cc.sh"

# ── 目标平台 ──
TARGETS=(
    "aarch64-apple-darwin"      # macOS Apple Silicon（宿主）
    "x86_64-apple-darwin"       # macOS Intel
    "x86_64-unknown-linux-musl" # Linux x86_64（静态链接）
    "aarch64-unknown-linux-musl" # Linux ARM64（静态链接）
    "x86_64-pc-windows-gnu"     # Windows x86_64（MinGW）
)

# ── 确保所有 target 已安装 ──
for t in "${TARGETS[@]}"; do
    rustup target add "$t" 2>&1 | grep -v "up to date" || true
done

# ── 配置 .cargo/config.toml ──
mkdir -p .cargo
cat > .cargo/config.toml <<'EOF'
[target.aarch64-apple-darwin]
linker = "clang"

[target.x86_64-apple-darwin]
linker = "clang"

[target.x86_64-unknown-linux-musl]
linker = "rust-lld"
rustflags = ["-C", "link-self-contained=yes"]

[target.aarch64-unknown-linux-musl]
linker = "rust-lld"
rustflags = ["-C", "link-self-contained=yes"]

[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
EOF

# ── 编译 ──
for t in "${TARGETS[@]}"; do
    echo "==> Building $t ..."
    # 非 Apple 目标需要用 zig cc 作为 C 编译器（编译 runtime.c）
    case "$t" in
        aarch64-apple-darwin|x86_64-apple-darwin)
            cargo build --release --target "$t" 2>&1 | tail -3
            ;;
        *)
            CC_ENV="CC_$(echo "$t" | tr '[:upper:]-' '[:lower:]_' | sed 's/\./_/g' | tr '-' '_')"
    export "$CC_ENV=$ZIG_CC"
            cargo build --release --target "$t" 2>&1 | tail -3
            ;;
    esac
    echo "==> $t done"
done

# ── 产物汇总 ──
echo ""
echo "========== Build artifacts =========="
for t in "${TARGETS[@]}"; do
    case "$t" in
        x86_64-pc-windows-gnu) bin="target/$t/release/nupac.exe" ;;
        aarch64-apple-darwin)  bin="target/release/nupac" ;;  # 宿主目标
        *) bin="target/$t/release/nupac" ;;
    esac
    [ -f "$bin" ] && echo "$(ls -lh "$bin" | awk '{print $5}')  $bin" && file "$bin" | sed 's/.*: //' && echo ""
done