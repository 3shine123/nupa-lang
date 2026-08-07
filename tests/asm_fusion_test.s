    .text
    .balign 4

// ─────────────────────────────────────────────────────────────
// unsigned asm_crc32(const unsigned char *buf, int len)
// CRC-32 (IEEE 802.3, reflected poly 0xEDB88320)
//   crc = 0xFFFFFFFF;  crc ^= byte;
//   8x { crc = (crc&1) ? (crc>>1)^0xEDB88320 : crc>>1 }
//   return ~crc
// Known vectors: crc32("123456789")=0xCBF43926  crc32("hello")=0x3610A686
// ─────────────────────────────────────────────────────────────
    .globl _asm_crc32
_asm_crc32:
    movz w5, #0xEDB8, lsl #16
    movk w5, #0x8320              // w5 = 0xEDB88320
    mov  w2, #0xFFFFFFFF          // crc
.Lcrc_loop:
    cbz  w1, .Lcrc_done
    ldrb w3, [x0], #1             // byte = *buf++
    eor  w2, w2, w3               // crc ^= byte
    mov  w4, #8                   // bit counter
.Lcrc_bits:
    tst  w2, #1
    b.eq .Lcrc_no_poly
    lsr  w2, w2, #1
    eor  w2, w2, w5
    b    .Lcrc_next
.Lcrc_no_poly:
    lsr  w2, w2, #1
.Lcrc_next:
    subs w4, w4, #1
    b.ne .Lcrc_bits
    sub  w1, w1, #1
    b    .Lcrc_loop
.Lcrc_done:
    mvn  w0, w2                   // ~crc
    ret

// ─────────────────────────────────────────────────────────────
// unsigned asm_rotl32(unsigned x, int n)   — rotate left by (n & 31)
// ─────────────────────────────────────────────────────────────
    .globl _asm_rotl32
_asm_rotl32:
    and  w2, w1, #31
    neg  w2, w2                   // -n  =>  ror (32-n)
    ror  w0, w0, w2
    ret

// ─────────────────────────────────────────────────────────────
// unsigned asm_clz32(unsigned x)  — count leading zero bits
// ─────────────────────────────────────────────────────────────
    .globl _asm_clz32
_asm_clz32:
    clz  w0, w0
    ret

// ─────────────────────────────────────────────────────────────
// unsigned asm_bitrev32(unsigned x)  — reverse all 32 bits
// ─────────────────────────────────────────────────────────────
    .globl _asm_bitrev32
_asm_bitrev32:
    rbit  w0, w0
    ret
