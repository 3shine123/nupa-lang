#!/bin/bash
# compile.sh — 逐条编译 mega_fusion 多文件联编测试
# 用法：./compile.sh
set -euo pipefail
cd "$(dirname "$0")"

echo "========================================"
echo "  Step 1: 转译 Nupa 代码 → C"
echo "========================================"
echo "  nupac -rewrite-nupa -fno-nupa-arc mega_fusion.np -o /tmp/mega_fusion.c"
echo ""
../../target/release/nupac -rewrite-nupa -fno-nupa-arc mega_fusion.np -o /tmp/mega_fusion.c
echo "  ✓ 转译完成"
echo ""

echo "========================================"
echo "  Step 2: 编译 + 链接（C + 汇编 + 运行时）"
echo "========================================"
echo "  clang -std=c99 -fblocks \\"
echo "    -I . -I ../../include \\"
echo "    /tmp/mega_fusion.c \\"
echo "    mega_fusion.c \\"
echo "    mega_fusion.s \\"
echo "    ../../include/nupa/runtime.c \\"
echo "    -o /tmp/mega_fusion"
echo ""
clang -std=c99 -fblocks \
  -I . -I ../../include \
  /tmp/mega_fusion.c \
  mega_fusion.c \
  mega_fusion.s \
  ../../include/nupa/runtime.c \
  -o /tmp/mega_fusion
echo "  ✓ 编译链接完成"
echo ""

echo "========================================"
echo "  Step 3: 运行测试"
echo "========================================"
/tmp/mega_fusion
echo ""

echo "========================================"
echo "  Step 4: ASAN 编译 + 运行（内存错误检测）"
echo "========================================"
echo "  clang -std=c99 -fblocks -fsanitize=address -g \\"
echo "    -I . -I ../../include \\"
echo "    /tmp/mega_fusion.c \\"
echo "    mega_fusion.c \\"
echo "    mega_fusion.s \\"
echo "    ../../include/nupa/runtime.c \\"
echo "    -o /tmp/mega_fusion_asan"
echo ""
clang -std=c99 -fblocks -fsanitize=address -g \
  -I . -I ../../include \
  /tmp/mega_fusion.c \
  mega_fusion.c \
  mega_fusion.s \
  ../../include/nupa/runtime.c \
  -o /tmp/mega_fusion_asan 2>&1 | grep -c warning
echo "  ✓ ASAN 编译完成"
echo ""
/tmp/mega_fusion_asan 2>&1
echo ""
echo "exit=$?"