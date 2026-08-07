#include <stdint.h>
#include "kernel.h"

/* ================= global tick counter (IRQ0) ================= */

volatile uint32_t tick = 0;

/* ================= IDT ================= */

struct idt_entry {
    uint16_t lo;
    uint16_t sel;
    uint8_t  zero;
    uint8_t  attr;
    uint16_t hi;
} __attribute__((packed));

struct idt_ptr {
    uint16_t limit;
    uint32_t base;
} __attribute__((packed));

static struct idt_entry idt[256];
static struct idt_ptr   idtp;

/* isr_stubs[] is defined in isr.asm: addresses of isr0..isr47 */
extern void (*const isr_stubs[])(void);

static void idt_set(uint8_t n, void (*handler)(void)) {
    uint32_t addr = (uint32_t)(uintptr_t)handler;
    idt[n].lo   = (uint16_t)(addr & 0xFFFF);
    idt[n].sel  = 0x08;
    idt[n].zero = 0;
    idt[n].attr = 0x8E; /* present, DPL0, 32-bit interrupt gate */
    idt[n].hi   = (uint16_t)((addr >> 16) & 0xFFFF);
}

void idt_init(void) {
    int i;
    for (i = 0; i < 48; i++) idt_set((uint8_t)i, isr_stubs[i]);
    idtp.limit = (uint16_t)(sizeof(idt) - 1);
    idtp.base  = (uint32_t)(uintptr_t)idt;
    asm volatile("lidt %0" : : "m"(idtp));
}

/* ================= ISR handler (C side) ================= */

void isr_handler(uint32_t num, uint32_t err) {
    if (num == 0x20) {           /* PIT timer */
        tick++;
        outb(0x20, 0x20);        /* EOI master */
        return;
    }
    if (num == 0x21) {           /* keyboard */
        uint8_t scancode = inb(0x60);
        kbd_handle(scancode);
        outb(0x20, 0x20);
        return;
    }
    kprintf("\n[isr] #%u err=0x%x\n", num, err);
    if (num >= 0x28) outb(0xA0, 0x20); /* EOI slave */
    outb(0x20, 0x20);                  /* EOI master */
}

/* ================= keyboard (IRQ1) ================= */

#define KBD_BUF_SIZE 256
static volatile int      kbd_buf[KBD_BUF_SIZE];  /* int for normal + extended codes */
static volatile unsigned kbd_head = 0;
static volatile unsigned kbd_tail = 0;
static volatile int      kbd_shift_state = 0;

/* Extended key codes returned by kgetchar (> 127): */
#define KEY_LEFT   0x80
#define KEY_RIGHT  0x81
#define KEY_UP     0x82
#define KEY_DOWN   0x83

/* PC/AT scan code set 1 → ASCII (US layout). 0 = ignore.
 * Arrow keys: 0x48=Up, 0x4B=Left, 0x4D=Right, 0x50=Down (0xE0-prefixed). */
static const unsigned char kbd_normal[128] = {
    0,   27,  '1','2','3','4','5','6','7','8','9','0','-','=','\b','\t',
    'q','w','e','r','t','y','u','i','o','p','[',']','\n', 0,
    'a','s','d','f','g','h','j','k','l',';','\'','`', 0,'\\',
    'z','x','c','v','b','n','m',',','.','/', 0,'*',
    0, ' ', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    KEY_UP, 0, 0, KEY_LEFT, 0, KEY_RIGHT, 0, 0, KEY_DOWN, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0
};

static const unsigned char kbd_shift[128] = {
    0,   27,  '!','@','#','$','%','^','&','*','(',')','_','+','\b','\t',
    'Q','W','E','R','T','Y','U','I','O','P','{','}','\n', 0,
    'A','S','D','F','G','H','J','K','L',':','"','~', 0,'|',
    'Z','X','C','V','B','N','M','<','>','?', 0,'*',
    0, ' ', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    KEY_UP, 0, 0, KEY_LEFT, 0, KEY_RIGHT, 0, 0, KEY_DOWN, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0
};

static void kbd_push(int c) {
    unsigned next = (kbd_tail + 1) % KBD_BUF_SIZE;
    if (next == kbd_head) return;
    kbd_buf[kbd_tail] = c;
    kbd_tail = next;
}

void kbd_handle(uint8_t scancode) {
    uint8_t key = scancode & 0x7F;
    int released = scancode & 0x80;
    if (key == 0x2A || key == 0x36) {
        kbd_shift_state = !released;
        return;
    }
    if (released || key >= 128) return;
    int c = kbd_shift_state ? kbd_shift[key] : kbd_normal[key];
    if (c) kbd_push(c);
}

int kgetchar(void) {
    if (kbd_head == kbd_tail) return -1;
    int c = kbd_buf[kbd_head];
    kbd_head = (kbd_head + 1) % KBD_BUF_SIZE;
    return c;
}

int kgetchar_ready(void) {
    return kbd_head != kbd_tail;
}

unsigned ktick(void) {
    return tick;
}

/* ================= PIC remap (IRQ0-15 -> int 0x20-0x2F) ================= */

void pic_remap(void) {
    outb(0x20, 0x11); io_wait();
    outb(0xA0, 0x11); io_wait();
    outb(0x21, 0x20); io_wait();
    outb(0xA1, 0x28); io_wait();
    outb(0x21, 0x04); io_wait();
    outb(0xA1, 0x02); io_wait();
    outb(0x21, 0x01); io_wait();
    outb(0xA1, 0x01); io_wait();
    outb(0x21, 0xFC); io_wait(); /* unmask IRQ0 + IRQ1 only */
    outb(0xA1, 0xFF); io_wait();
}

/* ================= PIT (channel 0, rate generator, 100 Hz) ================= */

void pit_init(int hz) {
    uint32_t div = 1193182u / (uint32_t)hz;
    outb(0x43, 0x36);
    outb(0x40, (uint8_t)div);
    outb(0x40, (uint8_t)(div >> 8));
}
