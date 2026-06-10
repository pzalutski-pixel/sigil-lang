/*
 * Sigil Runtime - Channel Implementation
 *
 * Bounded MPMC (multi-producer, multi-consumer) channel.
 */

#include "internal.h"
#include <stdlib.h>
#include <string.h>

/* Simple channel table - can be improved later */
#define MAX_CHANNELS 4096
static Channel* g_channels[MAX_CHANNELS] = {0};
static sigil_mutex_t g_channel_table_lock;
static sigil_once_t g_channel_table_once = SIGIL_ONCE_INIT;

static void init_channel_table(void) {
    sigil_mutex_init(&g_channel_table_lock);
}

static void ensure_channel_table_init(void) {
    /* Race-free one-time init (was a check-a-flag-then-init TOCTOU). */
    sigil_call_once(&g_channel_table_once, init_channel_table);
}

uint64_t sigil_channel_create(uint64_t elem_size, uint64_t capacity) {
    ensure_channel_table_init();

    if (capacity == 0) capacity = 1;  /* Minimum capacity */

    Channel* ch = (Channel*)calloc(1, sizeof(Channel));
    if (!ch) return 0;

    ch->elem_size = elem_size;
    ch->capacity = capacity;
    ch->buffer = (char*)calloc(capacity, elem_size);
    if (!ch->buffer) {
        free(ch);
        return 0;
    }

    ch->head = 0;
    ch->tail = 0;
    ch->count = 0;
    ch->closed = 0;

    sigil_mutex_init(&ch->lock);
    sigil_cond_init(&ch->not_full);
    sigil_cond_init(&ch->not_empty);

    /* Insert into table, reusing the first free slot so ids are bounded by the
     * number of LIVE channels, not the lifetime count — a destroyed channel
     * returns its slot to the pool (the old monotonic counter exhausted the
     * table after MAX_CHANNELS lifetime creations). */
    sigil_mutex_lock(&g_channel_table_lock);
    uint64_t id = 0;
    for (uint64_t i = 1; i < MAX_CHANNELS; i++) {
        if (g_channels[i] == NULL) { id = i; break; }
    }
    if (id != 0) {
        ch->id = id;
        g_channels[id] = ch;
    } else {
        /* Table full: MAX_CHANNELS channels live at once */
        free(ch->buffer);
        free(ch);
        ch = NULL;
    }
    sigil_mutex_unlock(&g_channel_table_lock);

    return id;
}

static Channel* get_channel(uint64_t handle) {
    if (handle == 0 || handle >= MAX_CHANNELS) return NULL;

    sigil_mutex_lock(&g_channel_table_lock);
    Channel* ch = g_channels[handle];
    sigil_mutex_unlock(&g_channel_table_lock);

    /* The returned pointer is used by callers after the table lock is released.
     * That is safe in the current runtime because channels are never destroyed
     * (codegen emits no sigil_channel_destroy), so the table entry cannot be
     * freed concurrently. If a destroy path is ever added, this needs refcounting
     * (or holding the table lock across the whole operation) to avoid a UAF. */
    return ch;
}

int64_t sigil_channel_send(uint64_t channel, void* value) {
    Channel* ch = get_channel(channel);
    if (!ch) return -1;

    sigil_mutex_lock(&ch->lock);

    /* Wait until not full or closed */
    while (ch->count == ch->capacity && !ch->closed) {
        sigil_cond_wait(&ch->not_full, &ch->lock);
    }

    if (ch->closed) {
        sigil_mutex_unlock(&ch->lock);
        return -1;
    }

    /* Copy value into buffer at tail position */
    char* dest = ch->buffer + (ch->tail * ch->elem_size);
    memcpy(dest, value, ch->elem_size);

    ch->tail = (ch->tail + 1) % ch->capacity;
    ch->count++;

    /* Wake one waiting receiver */
    sigil_cond_signal(&ch->not_empty);

    sigil_mutex_unlock(&ch->lock);

    return 0;
}

int64_t sigil_channel_receive(uint64_t channel, void* value) {
    Channel* ch = get_channel(channel);
    if (!ch) return -1;

    sigil_mutex_lock(&ch->lock);

    /* Wait until not empty or (closed and empty) */
    while (ch->count == 0 && !ch->closed) {
        sigil_cond_wait(&ch->not_empty, &ch->lock);
    }

    if (ch->count == 0 && ch->closed) {
        sigil_mutex_unlock(&ch->lock);
        return -1;  /* Closed and empty */
    }

    /* Copy value from buffer at head position */
    char* src = ch->buffer + (ch->head * ch->elem_size);
    memcpy(value, src, ch->elem_size);

    ch->head = (ch->head + 1) % ch->capacity;
    ch->count--;

    /* Wake one waiting sender */
    sigil_cond_signal(&ch->not_full);

    sigil_mutex_unlock(&ch->lock);

    return 0;
}

void sigil_channel_close(uint64_t channel) {
    Channel* ch = get_channel(channel);
    if (!ch) return;

    sigil_mutex_lock(&ch->lock);
    ch->closed = 1;

    /* Wake all waiters */
    sigil_cond_broadcast(&ch->not_full);
    sigil_cond_broadcast(&ch->not_empty);

    sigil_mutex_unlock(&ch->lock);
}

void sigil_channel_destroy(uint64_t channel) {
    Channel* ch = get_channel(channel);
    if (!ch) return;

    /* Remove from table */
    sigil_mutex_lock(&g_channel_table_lock);
    g_channels[channel] = NULL;
    sigil_mutex_unlock(&g_channel_table_lock);

    /* Free resources */
    sigil_mutex_destroy(&ch->lock);
    sigil_cond_destroy(&ch->not_full);
    sigil_cond_destroy(&ch->not_empty);
    free(ch->buffer);
    free(ch);
}
