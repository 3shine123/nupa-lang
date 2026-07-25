#ifndef NUPA_RUNTIME_H
#define NUPA_RUNTIME_H

#include <stdint.h>
#include <stddef.h>
#include <stdarg.h>
#include <setjmp.h>

// ─── Public types (used by generated code) ─────────────────────────────────────

#include <stdbool.h>

typedef bool Bool;
typedef int BOOL;
#define YES  true
#define NO   false

typedef struct {
    const char *name;
    unsigned hash;
} SEL;

typedef struct NPClass NPClass;
typedef struct __nupa_root __nupa_root;
typedef struct NPObject NPObject;
typedef struct NPProtocol NPProtocol;
typedef NPObject *id;
typedef NPObject *nupa_id_t;

#ifndef __NUPA_ROOT_DEFINED
#define __NUPA_ROOT_DEFINED
struct __nupa_root {
    struct NPClass *isa;
    uint32_t retain_count;
};
#endif

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

// Exception globals (TLS for thread safety)
extern __thread jmp_buf __nupa_exception_buf;
extern __thread id __nupa_exception_value;

NPObject *nupa_retain(NPObject *obj);
void nupa_release(NPObject *obj);
NPObject *nupa_autorelease(NPObject *obj);

NPObject *nupa_alloc(NPClass *cls);
NPObject *nupa_init(NPObject *self);

// ─── Autorelease pool API ────────────────────────────────────────────────────────

typedef struct nupa_autoreleasepool nupa_autoreleasepool_t;
nupa_autoreleasepool_t *nupa_autoreleasepool_push(void);
void nupa_autoreleasepool_pop(nupa_autoreleasepool_t *pool);

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
void __NPLogv(const char *format, va_list args);

// ─── Block runtime (Clang Blocks ABI) ─────────────────────────────────────────

void *_Block_copy(const void *aBlock);
void _Block_release(const void *aBlock);

// ─── Weak reference API ─────────────────────────────────────────────────────────

void nupa_weak_register(NPObject **weak_loc, NPObject *target);
void nupa_weak_unregister(NPObject **weak_loc);
void nupa_weak_clear_all(NPObject *target);
void nupa_weak_auto_cleanup(void *ptr);

// ─── helpers ──────────────────────────────────────────────────────────────────

#define NP_OBJECT_ISA(obj)        (((np_object_t *)(obj))->isa)
#define NP_CLASS_NAME(cls)        ((cls)->name)
#define NP_CLASS_SUPER(cls)       ((cls)->superclass)
#define NP_VTABLE_LOOKUP(obj, idx)  ((obj)->isa->vtable->methods[(idx)])

#endif /* NUPA_OBJECT_H */