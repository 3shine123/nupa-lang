#include <nupa/runtime.h>
#include <stdlib.h>
#include <string.h>

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
