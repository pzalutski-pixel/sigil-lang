/*
 * Sigil Runtime - Internal Definitions
 *
 * Not exposed in public API. Used by runtime implementation.
 */

#ifndef SIGIL_INTERNAL_H
#define SIGIL_INTERNAL_H

#include "platform.h"
#include <stdint.h>
#include "../include/sigil_runtime.h"

/* ============================================================================
 * Task (Green Thread) Structure
 * ============================================================================ */

typedef struct Task {
    uint64_t id;                    /* Unique task identifier */
    TaskState state;                /* Current state */

    void (*func)(void*);            /* Behavior function to execute */
    void* args;                     /* Argument buffer (owned by task) */
    uint64_t args_size;             /* Size of args buffer */

    void* result;                   /* Result buffer (for WAIT) */
    uint64_t result_size;           /* Size of result buffer */

    sigil_cond_t completed;         /* Signaled when task completes */

    struct Task* next;              /* Queue linkage */
} Task;

/* ============================================================================
 * Scheduler (Work Queue)
 * ============================================================================ */

typedef struct Scheduler {
    Task* queue_head;               /* Head of task queue */
    Task* queue_tail;               /* Tail of task queue */

    sigil_mutex_t queue_lock;       /* Protects queue access */
    sigil_cond_t queue_not_empty;   /* Signaled when work available */

    volatile int shutdown;          /* Set to 1 to terminate workers */
} Scheduler;

/* ============================================================================
 * Task Table (Handle -> Task mapping)
 * ============================================================================ */

#define INITIAL_TASK_TABLE_SIZE 1024

typedef struct TaskTable {
    Task** tasks;                   /* Array of task pointers by ID */
    uint64_t capacity;              /* Current table capacity */

    sigil_mutex_t lock;             /* Protects table access */
    sigil_cond_t any_completed;     /* Broadcast when ANY task completes (WAIT_ANY) */
} TaskTable;

/* ============================================================================
 * Worker Thread Pool
 * ============================================================================ */

typedef struct WorkerPool {
    sigil_thread_t* threads;        /* Array of worker thread handles */
    uint32_t thread_count;          /* Number of worker threads */
} WorkerPool;

/* ============================================================================
 * Channel Structure
 * ============================================================================ */

typedef struct Channel {
    uint64_t id;                    /* Unique channel identifier */

    uint64_t elem_size;             /* Size of each element */
    uint64_t capacity;              /* Maximum elements */
    char* buffer;                   /* Circular buffer */

    uint64_t head;                  /* Read position */
    uint64_t tail;                  /* Write position */
    uint64_t count;                 /* Current element count */

    sigil_mutex_t lock;             /* Protects buffer access */
    sigil_cond_t not_full;          /* Signaled when space available */
    sigil_cond_t not_empty;         /* Signaled when data available */

    volatile int closed;            /* Set to 1 when closed */
} Channel;

/* ============================================================================
 * Global Runtime State
 * ============================================================================ */

typedef struct Runtime {
    Scheduler scheduler;
    TaskTable task_table;
    WorkerPool worker_pool;

    int initialized;
} Runtime;

/* Global runtime instance */
extern Runtime g_runtime;

/* ============================================================================
 * Internal Functions
 * ============================================================================ */

/* Task management */
Task* task_create(void (*func)(void*), void* args, uint64_t args_size);
void task_destroy(Task* task);
Task* task_table_get(uint64_t handle);
uint64_t task_table_insert(Task* task);
void task_table_remove(uint64_t handle);

/* Scheduler */
void scheduler_init(Scheduler* s);
void scheduler_shutdown(Scheduler* s);
void scheduler_enqueue(Scheduler* s, Task* task);
Task* scheduler_dequeue(Scheduler* s);  /* Blocks until task available or shutdown */

/* Worker pool */
void worker_pool_init(WorkerPool* pool, uint32_t thread_count);
void worker_pool_shutdown(WorkerPool* pool);
sigil_thread_ret_t SIGIL_THREAD_CALL worker_thread_func(void* param);

#endif /* SIGIL_INTERNAL_H */
