// nupac — Nupa Runtime
// nupa_class.c — Class registration and vtable management

#include "nupa/runtime.h"
#include <stdlib.h>
#include <string.h>
#include <stdio.h>

// ─── class registry (simple linked list) ─────────────────────────────────────

static np_class_t *class_list = NULL;

void np_class_register(np_class_t *cls) {
    cls->vtable = NULL;
    cls->constructor = NULL;
    cls->superclass = NULL;

    // Link into global list (prepend)
    // Simple approach: just store, no list needed
    (void)class_list;
}

np_class_t *np_class_create(const char *name, np_class_t *superclass, size_t instance_size) {
    np_class_t *cls = calloc(1, sizeof(np_class_t));
    cls->name = name ? strdup(name) : NULL;
    cls->superclass = superclass;
    cls->instance_size = instance_size;
    cls->vtable = NULL;
    cls->constructor = NULL;

    // Link into global list
    // (for future use by class lookup)
    return cls;
}

void np_class_set_vtable(np_class_t *cls, np_vtable_t *vtable) {
    cls->vtable = vtable;
}

np_vtable_t *np_vtable_alloc(int method_count) {
    np_vtable_t *vt = calloc(1, sizeof(np_vtable_t));
    vt->method_count = method_count;
    vt->methods = calloc(method_count, sizeof(void (*)(void)));
    return vt;
}

void np_vtable_set_method(np_vtable_t *vt, int index, void (*method)(void)) {
    if (index < vt->method_count)
        vt->methods[index] = method;
}

np_object_t *np_object_alloc(np_class_t *cls) {
    np_object_t *obj = calloc(1, cls->instance_size);
    obj->isa = cls;
    return obj;
}

void np_object_dealloc(np_object_t *obj) {
    free(obj);
}