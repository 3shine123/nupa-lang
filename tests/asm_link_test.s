    .text
    .globl _asm_square
_asm_square:
    mul x0, x0, x0
    ret

    .globl _asm_add3
_asm_add3:
    add x0, x0, x1
    add x0, x0, x2
    ret
