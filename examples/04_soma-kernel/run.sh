#!/usr/bin/env bash
# examples/04_soma-kernel/run.sh — build and boot the kernel in QEMU
#
#   ./run.sh         headless test: capture serial.log, verify qemu exits 33
#   ./run.sh --gui   open a native window (cocoa) showing the VGA text screen;
#                    kernel halts at the end and the window stays open
set -euo pipefail
cd "$(dirname "$0")"

GUI=0
[ "${1:-}" = "--gui" ] && GUI=1

make build/floppy.img >/dev/null

LOG=build/serial.log
rm -f "$LOG"

TIMEOUT_CMD=$(command -v timeout 2>/dev/null || command -v gtimeout 2>/dev/null || echo timeout)

if [ "$GUI" -eq 1 ]; then
    echo "== booting in qemu-system-i386 (graphical window; close it to quit) =="
    qemu-system-i386 -drive file=build/floppy.img,format=raw,if=floppy \
        -display cocoa \
        -serial stdio \
        -no-reboot
    exit 0
fi

echo "== booting in qemu-system-i386 =="
set +e
"$TIMEOUT_CMD" 30 \
    qemu-system-i386 -drive file=build/floppy.img,format=raw,if=floppy \
    -display none \
    -serial "file:$LOG" \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
    -no-reboot
ec=$?
set -e

echo "== qemu exit code: $ec (expected 33) =="
echo "=========================== serial.log ==========================="
cat "$LOG"
echo "================================================================="

if [ "$ec" -eq 33 ]; then
    echo "SOMA KERNEL TEST PASS"
    exit 0
fi
echo "FAIL: kernel did not complete (qemu exited $ec)"
exit 1
