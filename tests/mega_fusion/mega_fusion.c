// mega_fusion.c — C helpers for mega fusion test
#include "mega_fusion.h"
#include <stdio.h>
#include <stdlib.h>

void mem_stats_reset(MemStats *stats) {
    stats->total_allocs = 0;
    stats->total_frees = 0;
    stats->current_allocated = 0;
    stats->peak_allocated = 0;
}

void mem_stats_alloc(MemStats *stats, size_t size) {
    stats->total_allocs++;
    stats->current_allocated += (long)size;
    if (stats->current_allocated > stats->peak_allocated) {
        stats->peak_allocated = stats->current_allocated;
    }
}

void mem_stats_free(MemStats *stats, size_t size) {
    stats->total_frees++;
    stats->current_allocated -= (long)size;
}

void mem_stats_print(const MemStats *stats, const char *label) {
    printf("  [C mem] %s: allocs=%ld frees=%ld live=%ld peak=%ld\n",
           label,
           stats->total_allocs,
           stats->total_frees,
           stats->current_allocated,
           stats->peak_allocated);
}

int stress_alloc_free_loop(int count, int size) {
    int lives = 0;
    for (int i = 0; i < count; i++) {
        char *p = (char *)malloc(size);
        if (!p) break;
        p[0] = (char)i;
        if (i % 3 == 0) {
            free(p);
        } else {
            lives++;
        }
    }
    return lives;
}

int stress_fibonacci(int n) {
    if (n < 2) return n;
    return stress_fibonacci(n - 1) + stress_fibonacci(n - 2);
}

int stress_factorial(int n) {
    int r = 1;
    for (int i = 2; i <= n; i++) r *= i;
    return r;
}