/*
 * Sigil Runtime - Task (Green Thread) Management
 */

#include "internal.h"
#include <stdlib.h>
#include <string.h>

Task* task_create(void (*func)(void*), void* args, uint64_t args_size) {
    Task* task = (Task*)calloc(1, sizeof(Task));
    if (!task) return NULL;

    task->func = func;
    task->state = TASK_PENDING;

    /* Copy args if provided */
    if (args && args_size > 0) {
        task->args = malloc(args_size);
        if (!task->args) {
            free(task);
            return NULL;
        }
        memcpy(task->args, args, args_size);
        task->args_size = args_size;
    }

    sigil_cond_init(&task->completed);

    return task;
}

void task_destroy(Task* task) {
    if (!task) return;

    sigil_cond_destroy(&task->completed);

    if (task->args) {
        free(task->args);
    }
    /* Note: result buffer is owned by caller, don't free here */

    free(task);
}

uint64_t task_table_insert(Task* task) {
    TaskTable* table = &g_runtime.task_table;

    sigil_mutex_lock(&table->lock);

    /* Reuse the first free slot (id 0 is reserved for "invalid"), so the id
     * space and table size stay bounded by the number of LIVE tasks rather than
     * the lifetime count — task_table_remove returns slots to the pool. The old
     * monotonic next_id grew the table unboundedly over a long-running process. */
    uint64_t id = 0;
    for (uint64_t i = 1; i < table->capacity; i++) {
        if (table->tasks[i] == NULL) { id = i; break; }
    }

    if (id == 0) {
        /* All live slots in use — grow and take the first newly-added index. */
        uint64_t new_capacity = table->capacity * 2;
        Task** new_tasks = (Task**)realloc(
            table->tasks, new_capacity * sizeof(Task*));
        if (!new_tasks) {
            sigil_mutex_unlock(&table->lock);
            return 0;  /* Allocation failed */
        }
        /* Zero out new entries */
        memset(new_tasks + table->capacity, 0,
               (new_capacity - table->capacity) * sizeof(Task*));
        id = table->capacity;  /* first new slot (>= old capacity >= 1) */
        table->tasks = new_tasks;
        table->capacity = new_capacity;
    }

    task->id = id;
    table->tasks[id] = task;

    sigil_mutex_unlock(&table->lock);

    return id;
}

Task* task_table_get(uint64_t handle) {
    TaskTable* table = &g_runtime.task_table;

    sigil_mutex_lock(&table->lock);

    Task* task = NULL;
    if (handle > 0 && handle < table->capacity) {
        task = table->tasks[handle];
    }

    sigil_mutex_unlock(&table->lock);

    return task;
}

void task_table_remove(uint64_t handle) {
    TaskTable* table = &g_runtime.task_table;

    sigil_mutex_lock(&table->lock);

    if (handle > 0 && handle < table->capacity) {
        table->tasks[handle] = NULL;
    }

    sigil_mutex_unlock(&table->lock);
}

/* ============================================================================
 * Public API - Spawn and Wait
 * ============================================================================ */

uint64_t sigil_spawn(void (*func)(void*), void* args, uint64_t args_size) {
    /* Auto-init if needed */
    if (!g_runtime.initialized) {
        sigil_runtime_init();
    }

    /* Create task */
    Task* task = task_create(func, args, args_size);
    if (!task) {
        return 0;  /* Failed */
    }

    /* Insert into table to get handle */
    uint64_t handle = task_table_insert(task);
    if (handle == 0) {
        task_destroy(task);
        return 0;
    }

    /* Enqueue for execution */
    scheduler_enqueue(&g_runtime.scheduler, task);

    return handle;
}

void sigil_wait(uint64_t handle, void* result, uint64_t result_size) {
    TaskTable* table = &g_runtime.task_table;

    /* Hold table->lock across the whole get/wait/copy/reap so the task pointer
     * can't be removed and freed out from under us (the old code released the
     * lock between get and dereference — a use-after-free if a WAIT raced a
     * WAIT_ANY on the same handle). */
    sigil_mutex_lock(&table->lock);
    Task* task = (handle > 0 && handle < table->capacity) ? table->tasks[handle] : NULL;
    if (!task) {
        sigil_mutex_unlock(&table->lock);
        return;  /* Invalid handle */
    }

    while (task->state != TASK_COMPLETED && task->state != TASK_FAILED) {
        sigil_cond_wait(&task->completed, &table->lock);
    }

    if (result && result_size > 0 && task->result) {
        uint64_t copy_size = result_size < task->result_size
                            ? result_size : task->result_size;
        memcpy(result, task->result, copy_size);
    }

    /* Reap: clear the slot under the lock, free after unlocking. */
    table->tasks[handle] = NULL;
    sigil_mutex_unlock(&table->lock);
    task_destroy(task);
}

void sigil_wait_all(uint64_t* handles, uint64_t count,
                    void** results, uint64_t* result_sizes) {
    /* Wait for each task in order */
    for (uint64_t i = 0; i < count; i++) {
        void* result = results ? results[i] : NULL;
        uint64_t size = result_sizes ? result_sizes[i] : 0;
        sigil_wait(handles[i], result, size);
    }
}

uint64_t sigil_wait_any(uint64_t* handles, uint64_t count,
                        void* result, uint64_t result_size) {
    TaskTable* table = &g_runtime.task_table;

    /* Block on the table-wide completion condvar instead of busy-polling.
     * The table access is inlined here because we must hold table->lock across
     * the check-and-wait (task_table_get/remove would re-acquire it). */
    sigil_mutex_lock(&table->lock);
    while (1) {
        for (uint64_t i = 0; i < count; i++) {
            uint64_t h = handles[i];
            Task* task = (h > 0 && h < table->capacity) ? table->tasks[h] : NULL;
            if (!task) continue;  /* Invalid or already reaped */

            if (task->state == TASK_COMPLETED || task->state == TASK_FAILED) {
                if (result && result_size > 0 && task->result) {
                    uint64_t copy_size = result_size < task->result_size
                                        ? result_size : task->result_size;
                    memcpy(result, task->result, copy_size);
                }

                /* Reap this task: clear the slot under the lock, then free it
                 * after unlocking (task_destroy touches only the task itself). */
                table->tasks[h] = NULL;
                sigil_mutex_unlock(&table->lock);
                task_destroy(task);
                return i;
            }
        }

        /* None ready — wait until some task completes, then re-check. */
        sigil_cond_wait(&table->any_completed, &table->lock);
    }
}
