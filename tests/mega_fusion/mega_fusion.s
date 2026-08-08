// mega_asm.s — ARM64 assembly helpers for mega fusion test
// Called from Nupa via extern declarations.

.globl _mega_asm_add3
_mega_asm_add3:
    add w0, w0, w1
    add w0, w0, w2
    ret

.globl _mega_asm_mul4
_mega_asm_mul4:
    mul w0, w0, w1
    mul w0, w0, w2
    mul w0, w0, w3
    ret

.globl _mega_asm_checksum
_mega_asm_checksum:
    mov w2, #0
    cbz w1, 2f
1:  ldrb w3, [x0], #1
    add w2, w2, w3
    sub w1, w1, #1
    cbnz w1, 1b
2:  mov w0, w2
    ret