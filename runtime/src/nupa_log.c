#include <nupa/runtime.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdarg.h>
#include <inttypes.h>

// ─── Description callback ────────────────────────────────────────────────────
// Like __NSDescriptionWithLocaleFunc in Apple's NSLog

typedef const char *(*nupa_description_func_t)(void *obj);

static nupa_description_func_t nupa_description_callback = NULL;

void nupa_set_description_func(nupa_description_func_t func) {
    nupa_description_callback = func;
}

static const char *nupa_default_description(void *obj) {
    if (!obj) return "(null)";
    NPObject *o = (NPObject *)obj;
    const char *name = o->isa ? o->isa->name : "?";
    static char buf[128];
    snprintf(buf, sizeof(buf), "<%s: 0x%" PRIxPTR ">", name, (uintptr_t)obj);
    return buf;
}

// ─── Internal formatter ──────────────────────────────────────────────────────
// Corresponds to __NSLogStringFormatter in Apple's Foundation

static void nupa_format_and_output(const char *format, va_list args) {
    if (!format) return;
    for (const char *p = format; *p; p++) {
        if (*p == '%') {
            p++;
            switch (*p) {
                case '@': {
                    void *obj = va_arg(args, void *);
                    const char *desc = nupa_description_callback
                        ? nupa_description_callback(obj)
                        : nupa_default_description(obj);
                    fputs(desc ? desc : "(null)", stderr);
                    break;
                }
                case 'd': {
                    int val = va_arg(args, int);
                    fprintf(stderr, "%d", val);
                    break;
                }
                case 's': {
                    const char *s = va_arg(args, const char *);
                    fputs(s ? s : "(null)", stderr);
                    break;
                }
                case '%':
                    fputc('%', stderr);
                    break;
                case '\0':
                    fputc('%', stderr);
                    goto done;
                default:
                    fputc('%', stderr);
                    fputc(*p, stderr);
                    break;
            }
        } else {
            fputc(*p, stderr);
        }
    }
done:
    fputc('\n', stderr);
}

// ─── Public API ──────────────────────────────────────────────────────────────

void NPLog(const char *format, ...) {
    va_list args;
    va_start(args, format);
    nupa_format_and_output(format, args);
    va_end(args);
}

void __NPLogv(const char *format, va_list args) {
    nupa_format_and_output(format, args);
}