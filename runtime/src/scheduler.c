/*
 * Sigil Runtime - Scheduler (Work Queue)
 */

#include "internal.h"

void scheduler_init(Scheduler* s) {
    s->queue_head = NULL;
    s->queue_tail = NULL;
    s->shutdown = 0;

    sigil_mutex_init(&s->queue_lock);
    sigil_cond_init(&s->queue_not_empty);
}

void scheduler_shutdown(Scheduler* s) {
    sigil_mutex_lock(&s->queue_lock);

    /* Wake all workers waiting on queue */
    s->shutdown = 1;
    sigil_cond_broadcast(&s->queue_not_empty);

    /* Clear any remaining tasks */
    while (s->queue_head) {
        Task* task = s->queue_head;
        s->queue_head = task->next;
        /* Don't destroy - they're still in task table */
    }
    s->queue_tail = NULL;

    sigil_mutex_unlock(&s->queue_lock);
    sigil_mutex_destroy(&s->queue_lock);
    sigil_cond_destroy(&s->queue_not_empty);
}

void scheduler_enqueue(Scheduler* s, Task* task) {
    sigil_mutex_lock(&s->queue_lock);

    task->next = NULL;

    if (s->queue_tail) {
        s->queue_tail->next = task;
        s->queue_tail = task;
    } else {
        s->queue_head = task;
        s->queue_tail = task;
    }

    /* Wake one waiting worker */
    sigil_cond_signal(&s->queue_not_empty);

    sigil_mutex_unlock(&s->queue_lock);
}

Task* scheduler_dequeue(Scheduler* s) {
    sigil_mutex_lock(&s->queue_lock);

    /* Wait for work or shutdown */
    while (s->queue_head == NULL && !s->shutdown) {
        sigil_cond_wait(&s->queue_not_empty, &s->queue_lock);
    }

    if (s->shutdown && s->queue_head == NULL) {
        sigil_mutex_unlock(&s->queue_lock);
        return NULL;  /* Shutdown, no more work */
    }

    /* Dequeue head */
    Task* task = s->queue_head;
    if (task) {
        s->queue_head = task->next;
        if (s->queue_head == NULL) {
            s->queue_tail = NULL;
        }
        task->next = NULL;
    }

    sigil_mutex_unlock(&s->queue_lock);

    return task;
}
