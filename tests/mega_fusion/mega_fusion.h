// mega_helpers.h — C helper declarations for mega fusion test
#ifndef MEGA_HELPERS_H
#define MEGA_HELPERS_H

#include <stddef.h>

// Memory tracking
typedef struct {
    long total_allocs;
    long total_frees;
    long current_allocated;
    long peak_allocated;
} MemStats;

void mem_stats_reset(MemStats *stats);
void mem_stats_alloc(MemStats *stats, size_t size);
void mem_stats_free(MemStats *stats, size_t size);
void mem_stats_print(const MemStats *stats, const char *label);

// Stress helpers
int stress_alloc_free_loop(int count, int size);
int stress_fibonacci(int n);
int stress_factorial(int n);

#endif