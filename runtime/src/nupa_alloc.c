#include <nupa/runtime.h>
#include <stdlib.h>
#include <string.h>

// Simple selector table
#define MAX_SELECTORS 256
static const char *sel_table[MAX_SELECTORS];
static int sel_count = 0;

static unsigned fnv1a_hash(const char *str) {
    unsigned hash = 2166136261u;
    while (*str) {
        hash ^= (unsigned char)*str++;
        hash *= 16777619u;
    }
    return hash;
}

SEL sel_registerName(const char *name) {
    for (int i = 0; i < sel_count; i++) {
        if (strcmp(sel_table[i], name) == 0) {
            return (SEL){.name = sel_table[i], .hash = fnv1a_hash(name)};
        }
    }
    if (sel_count < MAX_SELECTORS) {
        sel_table[sel_count++] = name;
        return (SEL){.name = name, .hash = fnv1a_hash(name)};
    }
    return (SEL){.name = name, .hash = fnv1a_hash(name)};
}

NPObject *nupa_alloc(NPClass *cls) {
    NPObject *obj = calloc(1, cls->instance_size);
    if (!obj) return NULL;
    obj->isa = cls;
    obj->retain_count = 1;
    return obj;
}

NPObject *nupa_init(NPObject *self) {
    return self;
}