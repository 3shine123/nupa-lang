#include "nupa/runtime.h"
#include <stdlib.h>
#include <string.h>
#include <stdio.h>

// ─── Exception globals ────────────────────────────────────────────────────────

__thread jmp_buf __nupa_exception_buf;
__thread id __nupa_exception_value;

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

void nupa_weak_register(NPObject **weak_loc, NPObject *target) {
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

void nupa_weak_unregister(NPObject **weak_loc) {
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

void nupa_weak_clear_all(NPObject *target) {
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

void nupa_weak_auto_cleanup(void *ptr) {
    nupa_weak_unregister((NPObject **)ptr);
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
        nupa_weak_clear_all(obj);
        free(obj);
    }
}

NPObject *nupa_autorelease(NPObject *obj) {
    return obj;
}

// ─── Autorelease pool ─────────────────────────────────────────────────────────

struct nupa_autoreleasepool {
    struct nupa_autoreleasepool *next;
};

nupa_autoreleasepool_t *nupa_autoreleasepool_push(void) {
    nupa_autoreleasepool_t *pool = malloc(sizeof(nupa_autoreleasepool_t));
    if (pool) {
        pool->next = NULL;
    }
    return pool;
}

void nupa_autoreleasepool_pop(nupa_autoreleasepool_t *pool) {
    free(pool);
}

// ─── Logging ─────────────────────────────────────────────────────────────────

void NPLog(const char *format, ...) {
    (void)format;
}

void __NPLogv(const char *format, va_list args) {
    (void)format;
    (void)args;
}