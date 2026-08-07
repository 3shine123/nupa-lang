#ifndef SOMA_KERNEL_H
#define SOMA_KERNEL_H

#include <stdint.h>

/* ---- port I/O (static inline, visible to all TUs) ---- */
static inline uint8_t inb(uint16_t port) {
    uint8_t r;
    asm volatile("inb %w1, %b0" : "=a"(r) : "Nd"(port));
    return r;
}
static inline void outb(uint16_t port, uint8_t val) {
    asm volatile("outb %b0, %w1" : : "a"(val), "Nd"(port));
}
static inline uint16_t inw(uint16_t port) {
    uint16_t r;
    asm volatile("inw %w1, %w0" : "=a"(r) : "Nd"(port));
    return r;
}
static inline void outw(uint16_t port, uint16_t val) {
    asm volatile("outw %w0, %w1" : : "a"(val), "Nd"(port));
}
static inline uint32_t inl(uint16_t port) {
    uint32_t r;
    asm volatile("inl %w1, %0" : "=a"(r) : "Nd"(port));
    return r;
}
static inline void outl(uint16_t port, uint32_t val) {
    asm volatile("outl %0, %w1" : : "a"(val), "Nd"(port));
}
static inline void io_wait(void) {
    outb(0x80, 0);
}

/* ---- console (kernel.c) ---- */
void    kputc(char c);
void    kputs(const char *s);
void    kputdec(int v);
void    kputhex(unsigned int v);
void    kprintf(const char *fmt, ...);
void    vga_clear(void);
void    vga_set_attr(uint8_t attr);

/* ---- hardware init (hw.c) ---- */
extern volatile uint32_t tick;
void    serial_init(void);
void    idt_init(void);
void    pic_remap(void);
void    pit_init(int hz);
void    isr_handler(uint32_t num, uint32_t err);
void    kbd_handle(uint8_t scancode);
int     kgetchar(void);
int     kgetchar_ready(void);
unsigned ktick(void);
void    kcur_move(int dx);

#endif
