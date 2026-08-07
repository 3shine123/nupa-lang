#!/usr/bin/env bash
# install.sh — 本平台安装脚本（由 build-all.sh 自动复制到每个平台的 target/<triple>/release/ 下）
# 用法：./install.sh [PREFIX]
# 检测系统语言，中文环境显示中文，其他环境显示英语
set -euo pipefail

BUNDLE="$(cd "$(dirname "$0")" && pwd)"

# ── 语言检测（中文 → zh，其他 → en） ──
LANG_OK="$(printf '%s\n%s\n%s\n' "${LC_ALL:-}" "${LC_MESSAGES:-}" "${LANG:-}")"
if printf '%s' "$LANG_OK" | grep -qiE 'zh|cn|chinese'; then
    I18N="zh"
else
    I18N="en"
fi

# ── 二进制名（Windows 为 .exe） ──
BIN="nupac"
[ -f "$BUNDLE/nupac.exe" ] && BIN="nupac.exe"

# ── 默认安装前缀 ──
PREFIX="${1:-/opt/nupa}"

# ── 文案 ──
if [ "$I18N" = "zh" ]; then
    MSG_TITLE="Nupa 安装包"
    MSG_INSTALL_DIR="安装目录"
    MSG_LIB="静态库"
    MSG_INC="头文件"
    MSG_DONE="安装完成"
    MSG_PATH="如需在 PATH 使用"
    MSG_UNINSTALL="卸载"
else
    MSG_TITLE="Nupa Installer"
    MSG_INSTALL_DIR="Install directory"
    MSG_LIB="library"
    MSG_INC="headers"
    MSG_DONE="Installation complete"
    MSG_PATH="To use from PATH, run"
    MSG_UNINSTALL="Uninstall"
fi

echo "==> ${MSG_TITLE} (${BIN})"
echo "    ${MSG_INSTALL_DIR}: $PREFIX"
echo ""

install -d "$PREFIX/bin" "$PREFIX/lib" "$PREFIX/include"

# 二进制
install -m 755 "$BUNDLE/$BIN" "$PREFIX/bin/nupac"

# 静态库
if [ -f "$BUNDLE/libnupa.a" ]; then
    install -m 644 "$BUNDLE/libnupa.a" "$PREFIX/lib/libnupa.a"
    echo "    ${MSG_LIB}:  $PREFIX/lib/libnupa.a"
fi

# 头文件（libVec<...>、runtime.h 等）
cp -r "$BUNDLE/include/." "$PREFIX/include/"
echo "    ${MSG_INC}:   $PREFIX/include/"

echo ""
echo "========== ${MSG_DONE} =========="
echo "  binary:  $PREFIX/bin/nupac"
echo "  headers: $PREFIX/include/"
echo ""
echo "  ${MSG_PATH}: export PATH=\"$PREFIX/bin:\$PATH\""
echo "  ${MSG_UNINSTALL}: rm -rf $PREFIX"