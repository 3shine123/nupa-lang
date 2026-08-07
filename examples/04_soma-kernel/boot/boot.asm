; boot/boot.asm — stage-1 boot sector for Soma Kernel
; Real mode -> read kernel.bin (LBA 1..N) to 0x10000 -> A20 -> GDT -> PM -> jump
; Build: nasm -f bin -DKERNEL_SECTORS=<n> boot.asm

[org 0x7C00]
[bits 16]

KERNEL_LOAD equ 0x10000
KERNEL_SEG  equ (KERNEL_LOAD >> 4)      ; 0x1000

%ifndef KERNEL_SECTORS
%define KERNEL_SECTORS 64
%endif

start:
    cli
    xor  ax, ax
    mov  ds, ax
    mov  es, ax
    mov  ss, ax
    mov  sp, 0x7C00
    sti

    mov  [boot_drive], dl               ; BIOS hands us the boot drive in DL

    mov  si, msg_loading
    call print16

    ; reset disk controller
    xor  ax, ax
    mov  dl, [boot_drive]
    int  0x13
    jc   disk_error

    ; load KERNEL_SECTORS sectors starting at LBA 1 -> 0x1000:0x0000
    mov  ax, KERNEL_SEG
    mov  es, ax
    xor  bx, bx
    mov  dword [lba], 1
    mov  di, KERNEL_SECTORS

.read_loop:
    test di, di
    jz   read_done

    ; LBA -> CHS (18 spt, 2 heads)
    mov  eax, [lba]
    xor  edx, edx
    mov  ecx, 18
    div  ecx
    mov  byte [sector], dl
    inc  byte [sector]
    xor  edx, edx
    mov  ecx, 2
    div  ecx
    mov  byte [cylinder], al
    mov  byte [head], dl

    mov  ah, 0x02
    mov  al, 1
    mov  ch, [cylinder]
    mov  cl, [sector]
    mov  dh, [head]
    mov  dl, [boot_drive]
    int  0x13
    jc   disk_error

    add  bx, 512
    mov  eax, [lba]
    inc  eax
    mov  [lba], eax
    dec  di
    jmp  .read_loop

read_done:
    cli

    call enable_a20
    lgdt [gdt_desc]

    mov  eax, cr0
    or   al, 1
    mov  cr0, eax
    jmp  0x08:pm_entry

[bits 32]
pm_entry:
    mov  ax, 0x10
    mov  ds, ax
    mov  es, ax
    mov  fs, ax
    mov  gs, ax
    mov  ss, ax
    mov  esp, 0x9000

    mov  eax, KERNEL_LOAD
    call eax
.hang:
    hlt
    jmp  .hang

[bits 16]
; ---------- helpers ----------
print16:
    lodsb
    test al, al
    jz   .done
    mov  ah, 0x0E
    int  0x10
    jmp  print16
.done:
    ret

enable_a20:
    mov  ax, 0x2401                  ; try BIOS fast-gate
    int  0x15
    jnc  .done
    in   al, 0x92                    ; fallback: port 0x92 fast A20
    test al, 2
    jnz  .done
    or   al, 2
    and  al, 0xFE
    out  0x92, al
.done:
    ret

disk_error:
    mov  si, msg_error
    call print16
.halt:
    hlt
    jmp  .halt

; ---------- data ----------
boot_drive: db 0
lba:        dd 1
cylinder:   db 0
head:       db 0
sector:     db 0

msg_loading: db "SOMA: loading kernel...", 13, 10, 0
msg_error:   db "SOMA: disk read error!", 13, 10, 0

align 4
gdt_start:
    dq 0
    ; code selector 0x08: flat, ring0, 32-bit, 4K-granular
    dw 0xFFFF
    dw 0x0000
    db 0x00
    db 0x9A
    db 0xCF
    db 0x00
    ; data selector 0x10: same but read/write
    dw 0xFFFF
    dw 0x0000
    db 0x00
    db 0x92
    db 0xCF
    db 0x00
gdt_end:

gdt_desc:
    dw gdt_end - gdt_start - 1
    dd gdt_start

times 510 - ($ - $$) db 0
dw 0xAA55
