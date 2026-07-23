#include <nupa/runtime.h>
#include <stdlib.h>

// ─── Autorelease pool ────────────────────────────────────────────────────────────
// Simple linked-list pool: each pool is a stack of objects.
// When the pool is drained, each object gets a release message.

typedef struct nupa_pool_obj {
    NPObject *obj;
    struct nupa_pool_obj *next;
} nupa_pool_obj_t;

typedef struct nupa_autoreleasepool {
    nupa_pool_obj_t *objects;
    struct nupa_autoreleasepool *previous;
} nupa_autoreleasepool_t;

// Thread-local current pool (global for simplicity)
static __thread nupa_autoreleasepool_t *current_pool = NULL;

nupa_autoreleasepool_t *nupa_autoreleasepool_push(void) {
    nupa_autoreleasepool_t *pool = calloc(1, sizeof(nupa_autoreleasepool_t));
    if (!pool) return NULL;
    pool->previous = current_pool;
    current_pool = pool;
    return pool;
}

void nupa_autoreleasepool_pop(nupa_autoreleasepool_t *pool) {
    if (!pool) return;
    // Drain: release all objects
    nupa_pool_obj_t *obj = pool->objects;
    while (obj) {
        nupa_pool_obj_t *next = obj->next;
        if (obj->obj) nupa_release(obj->obj);
        free(obj);
        obj = next;
    }
    // Restore previous pool
    current_pool = pool->previous;
    free(pool);
}

NPObject *nupa_autorelease(NPObject *obj) {
    if (!current_pool) return obj;
    nupa_pool_obj_t *entry = malloc(sizeof(nupa_pool_obj_t));
    if (!entry) return obj;
    entry->obj = obj;
    entry->next = current_pool->objects;
    current_pool->objects = entry;
    return obj;
}
