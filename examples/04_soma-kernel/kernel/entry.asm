; kernel/entry.asm — 32-bit kernel entry point (linker start)
[bits 32]

section .text
global kernel_entry

extern kmain

kernel_entry:
    cli
    mov  esp, stack_top

    ; clear the BSS ourselves (linker script places .bss here;
    ; we don't rely on a C runtime to zero it)
    extern __bss_start
    extern __bss_end
    mov  edi, __bss_start
    mov  ecx, __bss_end
    sub  ecx, edi
    xor  eax, eax
    rep  stosb

    call kmain

.hang:
    hlt
    jmp  .hang

section .bss
stack_bottom:
    resb 16384
stack_top:
