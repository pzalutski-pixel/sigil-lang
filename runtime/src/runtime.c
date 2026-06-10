/*
 * Sigil Runtime - Initialization and Shutdown
 */

#include "internal.h"
#include <stdlib.h>

/* Global runtime instance */
Runtime g_runtime = {0};

void sigil_runtime_init(void) {
    if (g_runtime.initialized) {
        return;  /* Already initialized */
    }

    /* Initialize task table */
    g_runtime.task_table.capacity = INITIAL_TASK_TABLE_SIZE;
    g_runtime.task_table.tasks = (Task**)calloc(
        g_runtime.task_table.capacity, sizeof(Task*));
    sigil_mutex_init(&g_runtime.task_table.lock);
    sigil_cond_init(&g_runtime.task_table.any_completed);

    /* Initialize scheduler */
    scheduler_init(&g_runtime.scheduler);

    /* Get CPU count for worker threads */
    uint32_t thread_count = sigil_get_cpu_count();
    if (thread_count < 1) thread_count = 1;
    if (thread_count > 64) thread_count = 64;  /* Reasonable limit */

    /* Initialize worker pool */
    worker_pool_init(&g_runtime.worker_pool, thread_count);

    g_runtime.initialized = 1;
}

void sigil_runtime_shutdown(void) {
    if (!g_runtime.initialized) {
        return;  /* Not initialized */
    }

    /* Signal shutdown and stop workers */
    g_runtime.scheduler.shutdown = 1;
    sigil_cond_broadcast(&g_runtime.scheduler.queue_not_empty);

    /* Wait for workers to finish */
    worker_pool_shutdown(&g_runtime.worker_pool);

    /* Cleanup scheduler */
    scheduler_shutdown(&g_runtime.scheduler);

    /* Cleanup task table */
    sigil_mutex_lock(&g_runtime.task_table.lock);
    for (uint64_t i = 0; i < g_runtime.task_table.capacity; i++) {
        if (g_runtime.task_table.tasks[i]) {
            task_destroy(g_runtime.task_table.tasks[i]);
        }
    }
    free(g_runtime.task_table.tasks);
    sigil_mutex_unlock(&g_runtime.task_table.lock);
    sigil_cond_destroy(&g_runtime.task_table.any_completed);
    sigil_mutex_destroy(&g_runtime.task_table.lock);

    g_runtime.initialized = 0;
}

/* Note: Auto-shutdown is handled by main() or explicit call.
 * The runtime should be explicitly shut down, or will leak on exit.
 */
