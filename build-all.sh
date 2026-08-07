#!/usr/bin/env bash
# build-all.sh — 一键交叉编译 nupac 到 5 个目标平台
# 依赖：zig 0.14+（brew install zig），mingw-w64（brew install mingw-w64），
#       rustup 管理的 Rust
set -euo pipefail
cd "$(dirname "$0")"

source "$HOME/.cargo/env"
ZIG_CC="$(pwd)/zig-cc.sh"
ZIG_AR="$(pwd)/zig-ar.sh"

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
            AR_ENV="AR_$(echo "$t" | tr '[:upper:]-' '[:lower:]_' | sed 's/\./_/g' | tr '-' '_')"
            # macOS 宿主 ar 无法打包非 Mach-O 对象，musl 等目标改用 zig ar
            export "$AR_ENV=$ZIG_AR"
            cargo build --release --target "$t" 2>&1 | tail -3
            ;;
    esac
    echo "==> $t done"
done

# ── 为每个平台生成自包含安装包（install.sh + 头文件） ──
for t in "${TARGETS[@]}"; do
    case "$t" in
        aarch64-apple-darwin) out="target/release" ;;   # 宿主目标
        *) out="target/$t/release" ;;
    esac
    # 全量头文件（libFire、nupa runtime 等）打包到 release 目录
    mkdir -p "$out/include"
    cp -r include/. "$out/include/"
    # 拷贝 install.sh
    cp install-pkg.sh "$out/install.sh"
    chmod +x "$out/install.sh"
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

echo "========== Install bundles (binary + install.sh + headers) =========="
for t in "${TARGETS[@]}"; do
    case "$t" in
        aarch64-apple-darwin) out="target/release" ;;
        *) out="target/$t/release" ;;
    esac
    [ -f "$out/install.sh" ] && echo "  $out/  ($(du -sh "$out" | awk '{print $1}')) — 运行 ./install.sh 安装到 /opt/nupa"
done

# ── 打包成压缩包（放到 target/ 根目录） ──
pack_name() {
    case "$1" in
        aarch64-apple-darwin)       echo "nupa-aarch64-apple-darwin" ;;
        x86_64-apple-darwin)        echo "nupa-x86_64-apple-darwin" ;;
        x86_64-unknown-linux-musl)  echo "nupa-x86_64-unknown-linux-musl" ;;
        aarch64-unknown-linux-musl) echo "nupa-aarch64-unknown-linux-musl" ;;
        x86_64-pc-windows-gnu)      echo "nupa-x86_64-pc-windows-gnu" ;;
        *) echo "nupa-$1" ;;
    esac
}

echo ""
echo "========== Packing into target/ =========="
for t in "${TARGETS[@]}"; do
    name="$(pack_name "$t")"
    exe=0
    case "$t" in
        aarch64-apple-darwin) out="target/release" ;;
        x86_64-pc-windows-gnu) out="target/$t/release"; exe=1 ;;
        *) out="target/$t/release" ;;
    esac
    staging="target/pack/$name"
    rm -rf "$staging"
    mkdir -p "$staging"
    # 只打包必要内容：二进制 + install.sh + 静态库 + 头文件
    if [ "$exe" = "1" ]; then binname="nupac.exe"; else binname="nupac"; fi
    cp "$out/$binname" "$staging/$binname"
    cp "$out/install.sh" "$staging/"
    chmod +x "$staging/install.sh"
    [ -f "$out/libnupa.a" ] && cp "$out/libnupa.a" "$staging/"
    cp -r "$out/include" "$staging/include"
    find "$staging" -name ".DS_Store" -delete

    if [ "$exe" = "1" ]; then
        # Windows → zip
        arch_path="target/${name}.zip"
        (cd target/pack && zip -rq "../${name}.zip" "$name")
    else
        # Unix → tar.gz
        arch_path="target/${name}.tar.gz"
        tar -C target/pack -czf "$arch_path" "$name"
    fi
    echo "  $(ls -lh "$arch_path" | awk '{print $5}')  $arch_path"
    rm -rf "$staging"
done
rm -rf target/pack