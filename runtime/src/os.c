/*
 * Sigil Runtime - OS / process access (args, env, exit)
 *
 * All parameters are pointers per the Sigil calling convention.
 */

#include "internal.h"
#include <stdlib.h>
#include <string.h>

#ifdef SIGIL_WINDOWS
/* The UCRT exposes the process command line as __argc / __argv. */
extern int __argc;
extern char** __argv;
#else
/* POSIX would need main() to stash argc/argv; not wired yet. */
static int __argc = 0;
static char** __argv = 0;
#endif

/* OUTPUT count int 8 */
void sigil_arg_count(int64_t* count) {
    if (count) *count = (int64_t)__argc;
}

/* INPUT index int 8; OUTPUT arg bytes 65536; OUTPUT length int 8 */
void sigil_arg_get(int64_t* index, void* arg, int64_t* length) {
    int64_t i = index ? *index : -1;
    char* out = (char*)arg;
    if (i < 0 || i >= (int64_t)__argc) {
        if (out) out[0] = '\0';
        if (length) *length = 0;
        return;
    }
    const char* a = __argv[i];
    size_t n = strlen(a);
    if (n > 65535) n = 65535;
    if (out) { memcpy(out, a, n); out[n] = '\0'; }
    if (length) *length = (int64_t)n;
}

/* INPUT name bytes 256 (null-terminated); OUTPUT value bytes 65536; OUTPUT found int 8 */
void sigil_env_get(void* name, void* value, int64_t* found) {
    const char* nc = (const char*)name;
    const char* v = getenv(nc);
    char* out = (char*)value;
    if (!v) {
        if (out) out[0] = '\0';
        if (found) *found = 0;
        return;
    }
    size_t n = strlen(v);
    if (n > 65535) n = 65535;
    if (out) { memcpy(out, v, n); out[n] = '\0'; }
    if (found) *found = 1;
}

/* INPUT code int 8 - terminates the process, does not return */
void sigil_exit(int64_t* code) {
    exit(code ? (int)*code : 0);
}
