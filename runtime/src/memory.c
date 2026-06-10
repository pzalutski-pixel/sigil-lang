/*
 * Sigil Runtime - Handle Memory (scope allocator)
 *
 * Implements the language's memory model:
 *   - ALLOC      -> heap allocation registered in the current (innermost) scope
 *   - FREE       -> frees that allocation now and unregisters it
 *   - SCOPE      -> a child scope (a lifetime boundary)
 *   - END_SCOPE  -> frees every allocation still live in that scope
 *
 * A SigilScope is an explicit token threaded through generated code: each
 * behavior invocation creates a ROOT scope, and each nested SCOPE creates a
 * CHILD. Because the token is explicit (never global/thread-local), the
 * allocator is thread-correct on the worker pool with no locking: each running
 * behavior owns its own scope tree.
 *
 * Correctness of the tricky edges:
 *   - Loop containing a SCOPE: END_SCOPE frees that iteration's allocations
 *     immediately, so memory stays bounded (it is NOT deferred to return).
 *   - JUMP out of a scope before END_SCOPE: the orphaned child stays linked to
 *     its parent, and destroying an ancestor cascades into it, so nothing leaks
 *     on abnormal exit.
 *   - FREE of a handle owned by an outer scope: FREE searches up the parent
 *     chain for the pointer, frees it, and unregisters it there.
 *
 * Scopes nest LIFO, so a scope has at most one open child at a time.
 */

#include <stdlib.h>
#include <stdint.h>
#include <stdio.h>

#define SIGIL_SCOPE_INITIAL_CAP 8

typedef struct SigilScope {
    void** allocs;                 /* live allocation pointers made in this scope */
    uint64_t count;
    uint64_t capacity;
    struct SigilScope* parent;     /* enclosing scope, or NULL for a behavior root */
    struct SigilScope* open_child; /* a child not yet destroyed (LIFO: at most one) */
} SigilScope;

static void sigil_oom(const char* what) {
    fprintf(stderr, "sigil: out of memory (%s)\n", what);
    abort();
}

/* Create a scope. parent == NULL makes a behavior-root scope; otherwise a child
 * of `parent` (a nested SCOPE). */
SigilScope* sigil_scope_create(SigilScope* parent) {
    SigilScope* s = (SigilScope*)malloc(sizeof(SigilScope));
    if (!s) sigil_oom("scope");
    s->allocs = (void**)malloc(SIGIL_SCOPE_INITIAL_CAP * sizeof(void*));
    if (!s->allocs) sigil_oom("scope registry");
    s->count = 0;
    s->capacity = SIGIL_SCOPE_INITIAL_CAP;
    s->parent = parent;
    s->open_child = NULL;
    if (parent) parent->open_child = s;
    return s;
}

/* ALLOC: heap-allocate `size` bytes, owned by scope `s`. */
void* sigil_scope_alloc(SigilScope* s, uint64_t size) {
    void* p = malloc(size ? size : 1);  /* size 0 -> a valid unique 1-byte ptr */
    if (!p) sigil_oom("allocation");
    if (s->count == s->capacity) {
        uint64_t new_cap = s->capacity * 2;
        void** grown = (void**)realloc(s->allocs, new_cap * sizeof(void*));
        if (!grown) sigil_oom("scope registry growth");
        s->allocs = grown;
        s->capacity = new_cap;
    }
    s->allocs[s->count++] = p;
    return p;
}

/* FREE: release one allocation now and unregister it from whichever scope (this
 * one or an ancestor) owns it. Tolerant of an unknown pointer so a double-FREE
 * cannot crash the runtime; the compiler's double-free check rejects it at build
 * time. */
void sigil_scope_free(SigilScope* s, void* ptr) {
    if (!ptr) return;
    for (SigilScope* scope = s; scope; scope = scope->parent) {
        for (uint64_t i = 0; i < scope->count; i++) {
            if (scope->allocs[i] == ptr) {
                free(ptr);
                scope->allocs[i] = scope->allocs[scope->count - 1]; /* swap-remove */
                scope->count--;
                return;
            }
        }
    }
    /* Not found: already freed or not owned here — do nothing (defensive). */
}

/* END_SCOPE / behavior return: free everything still live in `s`, cascading
 * into any child orphaned by a JUMP, then free `s` and unlink it from its
 * parent. */
void sigil_scope_destroy(SigilScope* s) {
    if (!s) return;
    if (s->open_child) {
        sigil_scope_destroy(s->open_child);  /* JUMP-orphaned child: cascade */
    }
    for (uint64_t i = 0; i < s->count; i++) {
        free(s->allocs[i]);
    }
    free(s->allocs);
    if (s->parent && s->parent->open_child == s) {
        s->parent->open_child = NULL;
    }
    free(s);
}
