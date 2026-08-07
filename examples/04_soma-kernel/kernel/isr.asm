; kernel/isr.asm — IDT stub generation for interrupts 0..47
[bits 32]

section .text

extern isr_handler

; Macro: ISR with no error code
%macro ISR_NOERR 1
    global isr%1
isr%1:
    push  dword 0          ; fake error code
    push  dword %1         ; interrupt number
    jmp   isr_common_stub
%endmacro

; Macro: ISR with a CPU-pushed error code
%macro ISR_ERR 1
    global isr%1
isr%1:
    push  dword %1         ; interrupt number (error code already on stack)
    jmp   isr_common_stub
%endmacro

; Faults that push an error code: 8,10,11,12,13,14,17
ISR_ERR 8
ISR_ERR 10
ISR_ERR 11
ISR_ERR 12
ISR_ERR 13
ISR_ERR 14
ISR_ERR 17

; All the rest
%assign i 0
%rep 48
    %if i != 8 && i != 10 && i != 11 && i != 12 && i != 13 && i != 14 && i != 17
        global isr%+i
isr%+i:
        push  dword 0
        push  dword i
        jmp   isr_common_stub
    %endif
%assign i i + 1
%endrep

isr_common_stub:
    pusha                       ; [esp+0..+28] = EDI..EAX
    mov   ax, ds
    push  eax                   ; [esp+0] = ds, [esp+4]=EDI ... [esp+32]=EAX
    mov   ax, 0x10              ; kernel data segment
    mov   ds, ax
    mov   es, ax
    mov   fs, ax
    mov   gs, ax

    ; num at [esp+36], err at [esp+40]; pass as cdecl args (err first)
    mov   eax, [esp + 36]
    mov   ebx, [esp + 40]
    push  ebx
    push  eax
    call  isr_handler
    add   esp, 8                ; pop the two arguments

    pop   eax                   ; restore ds
    mov   ds, ax
    mov   es, ax
    mov   fs, ax
    mov   gs, ax
    popa
    add   esp, 8                ; drop interrupt number + error code
    iret

section .rodata
; Addresses of all 48 stubs, indexed by interrupt number
global isr_stubs
isr_stubs:
%assign i 0
%rep 48
    dd isr%+i
%assign i i + 1
%endrep
