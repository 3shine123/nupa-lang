// nupa/runtime_baremetal.c — Freestanding runtime for bare-metal Nupa.
//
// Provides EVERYTHING the transpiled code needs on bare metal.
// Just link this file alongside the transpiled Nupa code — no hand-written
// globals, no memcpy, no runtime boilerplate.  The user only needs to
// provide freestanding headers (stdint.h, stddef.h, stdbool.h, string.h).
//
// Compile with -D__NUPA_FREESTANDING -I<nupac>/include.
// On the host (for testing), compile with -U__NUPA_FREESTANDING
// (uses system setjmp/longjmp, __thread globals).

#include <nupa/runtime.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

// ─── Runtime globals (referenced by transpiled code) ─────────────────────────

NPClass NUPA_CLASS_$_nupa_root;

#ifdef __NUPA_FREESTANDING
jmp_buf __nupa_exception_buf;
id      __nupa_exception_value;
#else
__thread jmp_buf __nupa_exception_buf;
__thread id      __nupa_exception_value;
#endif

// ─── memcpy (used by @try/@catch jmp_buf save/restore) ──────────────────────

#undef memcpy
__attribute__((weak)) void *memcpy(void *dst, const void *src, size_t n) {
    unsigned char *d = dst;
    const unsigned char *s = src;
    while (n--) *d++ = *s++;
    return dst;
}

// ─── Bump allocator ──────────────────────────────────────────────────────────

#ifndef NUPA_HEAP_SIZE
#define NUPA_HEAP_SIZE 16384
#endif

static char  nupa_heap[NUPA_HEAP_SIZE];
static size_t nupa_heap_off = 0;

void *nupa_malloc(size_t size) {
    size_t align = sizeof(size_t);
    size = (size + align - 1) & ~(align - 1);
    if (nupa_heap_off + size > NUPA_HEAP_SIZE) return NULL;
    void *p = &nupa_heap[nupa_heap_off];
    nupa_heap_off += size;
    return p;
}

void nupa_free(void *ptr) {
    (void)ptr; /* bump allocator: never reuses memory */
}

// ─── Autorelease pool ────────────────────────────────────────────────────────
// Bare-metal: single-core, no __thread.

struct nupa_autoreleasepool {
    struct nupa_autoreleasepool *next;
    NPObject **objects;
    int count;
    int capacity;
};

static nupa_autoreleasepool_t *current_pool = NULL;

nupa_autoreleasepool_t *nupa_autoreleasepoolPush(void) {
    nupa_autoreleasepool_t *pool = nupa_malloc(sizeof(nupa_autoreleasepool_t));
    if (!pool) return NULL;
    pool->next = current_pool;
    pool->objects = NULL;
    pool->count = 0;
    pool->capacity = 0;
    current_pool = pool;
    return pool;
}

void nupa_autoreleasepoolPop(nupa_autoreleasepool_t *pool) {
    if (!pool) return;
    for (int i = 0; i < pool->count; i++) {
        nupa_release(pool->objects[i]);
    }
    nupa_free(pool->objects);
    current_pool = pool->next;
    nupa_free(pool);
}

// ─── Lifecycle ───────────────────────────────────────────────────────────────

NPObject *nupa_alloc(NPClass *cls) {
    if (!cls) return NULL;
    NPObject *obj = (NPObject *)nupa_malloc(cls->instance_size);
    if (obj) {
        memset(obj, 0, cls->instance_size);
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
        // Call dealloc so ivar cleanup runs (e.g. NPString frees _cstr).
        // dealloc's own `[super dealloc]` calls the parent's dealloc directly
        // (not nupa_release), so no double-free.
        if (obj->isa && obj->isa->dealloc) {
            obj->isa->dealloc(obj, (SEL){ .name = "dealloc", .hash = 0xD9929EB3 });
        }
        nupa_free(obj);
    }
}

NPObject *nupa_autorelease(NPObject *obj) {
    if (!obj) return obj;
    nupa_autoreleasepool_t *pool = current_pool;
    if (!pool) return obj;
    if (pool->count >= pool->capacity) {
        int new_cap = pool->capacity ? pool->capacity * 2 : 16;
        NPObject **new_objs = nupa_malloc(new_cap * sizeof(NPObject *));
        if (!new_objs) return obj;
        if (pool->objects) {
            for (int i = 0; i < pool->count; i++) new_objs[i] = pool->objects[i];
        }
        pool->objects = new_objs;
        pool->capacity = new_cap;
    }
    pool->objects[pool->count++] = obj;
    return obj;
}