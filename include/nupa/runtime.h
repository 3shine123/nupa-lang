#ifndef NUPA_RUNTIME_H
#define NUPA_RUNTIME_H

/*
 * Nupa runtime header.
 *
 * Two modes:
 *   default               — host/OS mode: pulls in libc <setjmp.h>/<stdarg.h>,
 *                           exception state is thread-local (__thread).
 *   __NUPA_FREESTANDING   — bare-metal mode (set by `nupac -fno-libc`):
 *                           no libc headers; setjmp/longjmp map to Clang
 *                           builtins; exception state is plain globals
 *                           (single-core assumption). The user supplies
 *                           <stdint.h>/<stddef.h>/<stdbool.h> (freestanding).
 */

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

#ifdef __NUPA_FREESTANDING
typedef void *jmp_buf[16];
#define setjmp(env)         __builtin_setjmp(env)
#define longjmp(env, val)   __builtin_longjmp((env), (val))
#else
#include <stdarg.h>
#include <setjmp.h>
#endif

// ─── Public types (used by generated code) ─────────────────────────────────────

typedef bool Bool;
typedef int BOOL;
#define YES  true
#define NO   false

typedef struct {
    const char *name;
    unsigned hash;
} SEL;

typedef struct NPClass NPClass;
typedef struct nupa_root nupa_root;
typedef struct NPObject NPObject;
typedef NPObject *id;
typedef NPObject *nupa_id_t;

/* Always declared: the transpiler's nupa_metaInit() references it.
 * Definition comes from the user (freestanding) or Foundation (host). */
extern NPClass NUPA_CLASS_$_nupa_root;

/* memcpy is used by the @try/@catch @finally jmp_buf save/restore
 * (the generated code always calls memcpy for nesting save/restore).
 * Guard against macOS's fortified memcpy macro. */
#ifndef memcpy
void *memcpy(void *dst, const void *src, size_t n);
#endif

#ifdef __NUPA_FREESTANDING

#ifndef NUPA_ROOT_DEFINED
#define NUPA_ROOT_DEFINED
struct nupa_root {
    struct NPClass *isa;
    uint32_t retain_count;
};
#endif

#endif /* end of __NUPA_FREESTANDING guarded structs */

#ifndef NPOBJECT_DEFINED
#define NPOBJECT_DEFINED
struct NPObject {
    struct NPClass *isa;
    uint32_t retain_count;
};
#endif

struct NPClass {
    const char *name;
    NPClass *superclass;
    size_t instance_size;
    void *vtable;
    void *class_vtable;
    struct NPProtocol **protocols;
    int protocol_count;
    /* Populated by nupa_metaInit() (codegen). nupa_release() calls it when the
     * retain count reaches 0, so per-class dealloc cleanup (free-ing ivars)
     * actually runs. NULL if the class defines no instance dealloc. */
    void (*dealloc)(NPObject *, SEL);
};

// ─── Protocol types ──────────────────────────────────────────────────────────

typedef struct NPProtocolMethod {
    const char *name;
    const char *encoding;
} NPProtocolMethod;

struct NPProtocol {
    const char *name;
    struct NPProtocol **parents;
    int parent_count;
    NPProtocolMethod *required_methods;
    int required_count;
    NPProtocolMethod *optional_methods;
    int optional_count;
};

// ─── Internal types (for runtime implementation) ────────────────────────────────

typedef struct np_vtable np_vtable_t;
typedef struct np_class  np_class_t;
typedef struct np_object np_object_t;

struct np_vtable {
    np_class_t *isa;
    void      (**methods)(void);
    int        method_count;
};

struct np_class {
    np_class_t  *superclass;
    const char  *name;
    size_t       instance_size;
    np_vtable_t *vtable;
    void       (*constructor)(np_object_t *self, ...);
};

struct np_object {
    np_class_t *isa;
};

// ─── Runtime API ────────────────────────────────────────────────────────────────

SEL sel_registerName(const char *name);

/* Memory allocator (user-provided in freestanding; libc calloc/free on host).
 * nupa_malloc must zero-initialize memory. */
void *nupa_malloc(size_t size);
void  nupa_free(void *ptr);

NPObject *nupa_alloc(NPClass *cls);
NPObject *nupa_init(NPObject *self);

// Exception globals (TLS for thread safety; plain globals in freestanding)
#ifdef __NUPA_FREESTANDING
extern jmp_buf __nupa_exception_buf;
extern id     __nupa_exception_value;
#else
extern __thread jmp_buf __nupa_exception_buf;
extern __thread id     __nupa_exception_value;
#endif

NPObject *nupa_retain(NPObject *obj);
void nupa_release(NPObject *obj);
NPObject *nupa_autorelease(NPObject *obj);
BOOL nupa_isKindOf(NPObject *obj, NPClass *cls);

// ─── Autorelease pool API ────────────────────────────────────────────────────────

typedef struct nupa_autoreleasepool nupa_autoreleasepool_t;
nupa_autoreleasepool_t *nupa_autoreleasepoolPush(void);
void nupa_autoreleasepoolPop(nupa_autoreleasepool_t *pool);

// ─── Internal API (for runtime implementation) ───────────────────────────────────

void np_class_register(np_class_t *cls);
np_class_t *np_class_create(const char *name, np_class_t *superclass, size_t instance_size);
void np_class_set_vtable(np_class_t *cls, np_vtable_t *vtable);
np_vtable_t *np_vtable_alloc(int method_count);
void np_vtable_set_method(np_vtable_t *vt, int index, void (*method)(void));
np_object_t *np_object_alloc(np_class_t *cls);
void np_object_dealloc(np_object_t *obj);

// ─── Logging (like NSLog / NSObjCRuntime.h) ───────────────────────────────────

void NPLog(const char *format, ...);
#ifndef __NUPA_FREESTANDING
void __NPLogv(const char *format, va_list args);
#endif

// ─── Block runtime (Clang Blocks ABI) ─────────────────────────────────────────

void *_Block_copy(const void *aBlock);
void _Block_release(const void *aBlock);

// ─── Weak reference API ─────────────────────────────────────────────────────────

void nupa_weakRegister(NPObject **weak_loc, NPObject *target);
void nupa_weakUnregister(NPObject **weak_loc);
void nupa_weakClearAll(NPObject *target);

// ─── String literals ──────────────────────────────────────────────────────────

NPObject *nupa_stringFromCstr(const char *cstr);
void nupa_weakAutoCleanup(void *ptr);

// ─── helpers ──────────────────────────────────────────────────────────────────

#define NP_OBJECT_ISA(obj)        (((np_object_t *)(obj))->isa)
#define NP_CLASS_NAME(cls)        ((cls)->name)
#define NP_CLASS_SUPER(cls)       ((cls)->superclass)
#define NP_VTABLE_LOOKUP(obj, idx)  ((obj)->isa->vtable->methods[(idx)])

#endif /* NUPA_OBJECT_H */