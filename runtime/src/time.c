/*
 * Sigil Runtime - Time Operations
 *
 * Provides cross-platform time functions for NATIVE behaviors.
 *
 * Cross-platform implementation:
 * - Windows: GetSystemTimeAsFileTime, Sleep
 * - Linux/macOS: gettimeofday, nanosleep
 */

#include "internal.h"

#ifdef SIGIL_WINDOWS
    /* Windows already included via platform.h */
#else
    #include <sys/time.h>
    #include <time.h>
#endif

/*
 * Get current time in milliseconds since Unix epoch.
 *
 * timestamp: Output - milliseconds since 1970-01-01 00:00:00 UTC (pointer to int64)
 *
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_now(int64_t* timestamp) {
#ifdef SIGIL_WINDOWS
    FILETIME ft;
    ULARGE_INTEGER uli;

    GetSystemTimeAsFileTime(&ft);
    uli.LowPart = ft.dwLowDateTime;
    uli.HighPart = ft.dwHighDateTime;

    /* Convert from Windows epoch (1601) to Unix epoch (1970) */
    /* Windows FILETIME is in 100-nanosecond intervals */
    /* Difference: 11644473600 seconds */
    int64_t windows_ticks = (int64_t)uli.QuadPart;
    int64_t unix_ms = (windows_ticks / 10000) - 11644473600000LL;

    if (timestamp) *timestamp = unix_ms;
#else
    struct timeval tv;
    if (gettimeofday(&tv, NULL) != 0) {
        if (timestamp) *timestamp = 0;
        return;
    }

    int64_t ms = (int64_t)tv.tv_sec * 1000 + (int64_t)tv.tv_usec / 1000;
    if (timestamp) *timestamp = ms;
#endif
}

/*
 * Sleep for specified milliseconds.
 *
 * milliseconds: Duration to sleep (pointer to int64)
 * status: Output - 0 on success, -1 on error (pointer to int64)
 *
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_sleep(int64_t* milliseconds, int64_t* status) {
    int64_t ms = *milliseconds;
#ifdef SIGIL_WINDOWS
    Sleep((DWORD)ms);
    if (status) *status = 0;
#else
    struct timespec ts;
    ts.tv_sec = ms / 1000;
    ts.tv_nsec = (ms % 1000) * 1000000;

    int result = nanosleep(&ts, NULL);
    if (status) *status = (result == 0) ? 0 : -1;
#endif
}
