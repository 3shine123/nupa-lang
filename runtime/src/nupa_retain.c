#include <nupa/runtime.h>
#include <stdlib.h>

NPObject *nupa_retain(NPObject *obj) {
    if (!obj) return NULL;
    obj->retain_count++;
    return obj;
}