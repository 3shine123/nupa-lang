#include "nupa/runtime.h"
#include <stdlib.h>
#include <string.h>
#include <stdio.h>

// ─── NUPA_CLASS_$_nupa_root (defined weak; codegen's nupa_metaInit fills it) ──

NPClass NUPA_CLASS_$_nupa_root;

// ─── Exception globals ────────────────────────────────────────────────────────

#ifdef __NUPA_FREESTANDING
jmp_buf __nupa_exception_buf;
id      __nupa_exception_value;
#else
__thread jmp_buf __nupa_exception_buf;
__thread id     __nupa_exception_value;
#endif

// ─── Weak reference side table ───────────────────────────────────────────────

#define MAX_WEAK_ENTRIES 1024
#define INITIAL_SLOT_CAPACITY 4

typedef struct {
    NPObject *object;
    NPObject ***slots;
    int count;
    int capacity;
} WeakEntry;

static WeakEntry weak_table[MAX_WEAK_ENTRIES];
static int weak_entries = 0;

static WeakEntry *find_entry(NPObject *target) {
    for (int i = 0; i < weak_entries; i++) {
        if (weak_table[i].object == target)
            return &weak_table[i];
    }
    return NULL;
}

void nupa_weakRegister(NPObject **weak_loc, NPObject *target) {
    if (!target || !weak_loc) return;
    WeakEntry *entry = find_entry(target);
    if (!entry) {
        if (weak_entries >= MAX_WEAK_ENTRIES) return;
        entry = &weak_table[weak_entries++];
        entry->object = target;
        entry->slots = malloc(INITIAL_SLOT_CAPACITY * sizeof(NPObject **));
        entry->count = 0;
        entry->capacity = INITIAL_SLOT_CAPACITY;
    }
    if (entry->count >= entry->capacity) {
        entry->capacity *= 2;
        entry->slots = realloc(entry->slots, entry->capacity * sizeof(NPObject **));
    }
    entry->slots[entry->count++] = weak_loc;
}

void nupa_weakUnregister(NPObject **weak_loc) {
    if (!weak_loc) return;
    for (int i = 0; i < weak_entries; i++) {
        WeakEntry *entry = &weak_table[i];
        for (int j = 0; j < entry->count; j++) {
            if (entry->slots[j] == weak_loc) {
                entry->slots[j] = entry->slots[--entry->count];
                return;
            }
        }
    }
}

void nupa_weakClearAll(NPObject *target) {
    if (!target) return;
    for (int i = 0; i < weak_entries; i++) {
        WeakEntry *entry = &weak_table[i];
        if (entry->object == target) {
            for (int j = 0; j < entry->count; j++) {
                *entry->slots[j] = NULL;
            }
            free(entry->slots);
            weak_table[i] = weak_table[--weak_entries];
            return;
        }
    }
}

void nupa_weakAutoCleanup(void *ptr) {
    nupa_weakUnregister((NPObject **)ptr);
}

// ─── Selectors ───────────────────────────────────────────────────────────────

SEL sel_registerName(const char *name) {
    unsigned hash = 0x811C9DC5;
    for (const char *p = name; *p; p++) {
        hash ^= (unsigned char)*p;
        hash *= 0x01000193;
    }
    SEL sel = { name, hash };
    return sel;
}

// ─── Type introspection ─────────────────────────────────────────────────────────

BOOL nupa_isKindOf(NPObject *obj, NPClass *cls) {
    if (!obj || !cls) return 0;
    NPClass *isa = obj->isa;
    while (isa) {
        if (isa == cls) return 1;
        isa = isa->superclass;
    }
    return 0;
}

// ─── Autorelease pool ─────────────────────────────────────────────────────────

struct nupa_autoreleasepool {
    struct nupa_autoreleasepool *next;
    NPObject **objects;
    int count;
    int capacity;
};

static __thread nupa_autoreleasepool_t *current_pool = NULL;

nupa_autoreleasepool_t *nupa_autoreleasepoolPush(void) {
    nupa_autoreleasepool_t *pool = calloc(1, sizeof(nupa_autoreleasepool_t));
    if (!pool) return NULL;
    pool->next = current_pool;
    current_pool = pool;
    return pool;
}

void nupa_autoreleasepoolPop(nupa_autoreleasepool_t *pool) {
    if (!pool) return;
    for (int i = 0; i < pool->count; i++) {
        nupa_release(pool->objects[i]);
    }
    free(pool->objects);
    current_pool = pool->next;
    free(pool);
}

// ─── Lifecycle ───────────────────────────────────────────────────────────────

NPObject *nupa_alloc(NPClass *cls) {
    if (!cls) return NULL;
    NPObject *obj = (NPObject *)calloc(1, cls->instance_size);
    if (obj) {
        obj->isa = cls;
        obj->retain_count = 1;
    }
    return obj;
}

NPObject *nupa_init(NPObject *self) {
    return self;
}

NPObject *nupa_retain(NPObject *obj) {
    if (!obj) return NULL;
    obj->retain_count++;
    return obj;
}

void nupa_release(NPObject *obj) {
    if (!obj) return;
    if (obj->retain_count > 0)
        obj->retain_count--;
    if (obj->retain_count == 0) {
        // Zero weak references BEFORE dealloc: dealloc may free other objects
        // (strong ivars) whose memory holds a weak slot pointing back to us
        // (e.g. a child's `__weak parent`). Zeroing first avoids a use-after-free.
        nupa_weakClearAll(obj);
        // Call dealloc so ivar cleanup runs. dealloc's `[super dealloc]` calls
        // the parent's dealloc directly (not nupa_release), so no double-free.
        if (obj->isa && obj->isa->dealloc) {
            obj->isa->dealloc(obj, (SEL){ .name = "dealloc", .hash = 0xD9929EB3 });
        }
        free(obj);
    }
}

NPObject *nupa_autorelease(NPObject *obj) {
    if (!obj) return obj;
    nupa_autoreleasepool_t *pool = current_pool;
    if (!pool) return obj;
    if (pool->count >= pool->capacity) {
        pool->capacity = pool->capacity ? pool->capacity * 2 : 16;
        pool->objects = realloc(pool->objects, pool->capacity * sizeof(NPObject *));
        if (!pool->objects) return obj;
    }
    pool->objects[pool->count++] = obj;
    return obj;
}

// ─── String literals ──────────────────────────────────────────────────────────
// nupa_stringFromCstr is emitted by the codegen in the generated C code.
// The runtime.h declaration is used by the generated code to call it.
// When NPString is not present, @"..." falls back to a regular C string literal.

// ─── Logging ─────────────────────────────────────────────────────────────────

void NPLog(const char *format, ...) {
    (void)format;
}

void __NPLogv(const char *format, va_list args) {
    (void)format;
    (void)args;
}