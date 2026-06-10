/*
 * Sigil Runtime - Worker Thread Pool
 */

#include "internal.h"
#include <stdlib.h>

sigil_thread_ret_t SIGIL_THREAD_CALL worker_thread_func(void* param) {
    (void)param;  /* Unused for now */

    while (1) {
        /* Get next task from scheduler */
        Task* task = scheduler_dequeue(&g_runtime.scheduler);

        if (task == NULL) {
            /* Shutdown signal received */
            break;
        }

        /* Update task state */
        sigil_mutex_lock(&g_runtime.task_table.lock);
        task->state = TASK_RUNNING;
        sigil_mutex_unlock(&g_runtime.task_table.lock);

        /* Execute the task */
        if (task->func) {
            task->func(task->args);
        }

        /* Mark as completed and signal waiters: the per-task condvar wakes a
         * WAIT on this handle; the table-wide condvar wakes any WAIT_ANY. */
        sigil_mutex_lock(&g_runtime.task_table.lock);
        task->state = TASK_COMPLETED;
        sigil_cond_broadcast(&task->completed);
        sigil_cond_broadcast(&g_runtime.task_table.any_completed);
        sigil_mutex_unlock(&g_runtime.task_table.lock);
    }

#ifdef SIGIL_WINDOWS
    return 0;
#else
    return NULL;
#endif
}

void worker_pool_init(WorkerPool* pool, uint32_t thread_count) {
    pool->thread_count = thread_count;
    pool->threads = (sigil_thread_t*)calloc(thread_count, sizeof(sigil_thread_t));

    for (uint32_t i = 0; i < thread_count; i++) {
        sigil_thread_create(&pool->threads[i], worker_thread_func, NULL);
    }
}

void worker_pool_shutdown(WorkerPool* pool) {
    /* Wait for all workers to finish */
    if (pool->threads && pool->thread_count > 0) {
        sigil_thread_join_multiple(pool->threads, pool->thread_count);

        free(pool->threads);
        pool->threads = NULL;
    }

    pool->thread_count = 0;
}
