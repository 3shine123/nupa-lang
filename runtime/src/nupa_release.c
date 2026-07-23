#include <nupa/runtime.h>
#include <stdlib.h>

void nupa_release(NPObject *obj) {
    if (!obj) return;
    if (obj->retain_count > 0)
        obj->retain_count--;
    if (obj->retain_count == 0) {
        nupa_weak_clear_all(obj);
        free(obj);
    }
}