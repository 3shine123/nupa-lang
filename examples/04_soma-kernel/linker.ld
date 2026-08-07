/* tests/soma-kernel/linker.ld — link kernel at physical 0x10000 */
ENTRY(kernel_entry)

SECTIONS
{
    . = 0x10000;

    .text : {
        *(.text)
    }

    .rodata : {
        *(.rodata*)
    }

    .data : {
        *(.data)
    }

    __bss_start = .;
    .bss : {
        *(.bss*)
        *(COMMON)
    }
    __bss_end = .;

    . = ALIGN(16);
    end = .;
}
