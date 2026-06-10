/*
 * Sigil Runtime - Platform Abstraction Layer
 *
 * Provides cross-platform abstractions for:
 * - Threading (threads, mutexes, condition variables)
 * - System information (CPU count)
 * - Networking (BSD sockets vs Winsock)
 * - Sleep
 *
 * Supported platforms:
 * - Windows (MSVC)
 * - Linux (GCC/Clang)
 * - macOS (Clang)
 */

#ifndef SIGIL_PLATFORM_H
#define SIGIL_PLATFORM_H

#include <stdint.h>

/* ============================================================================
 * Platform Detection
 * ============================================================================ */

#if defined(_WIN32) || defined(_WIN64)
    #define SIGIL_WINDOWS 1
#elif defined(__linux__)
    #define SIGIL_LINUX 1
#elif defined(__APPLE__) && defined(__MACH__)
    #define SIGIL_MACOS 1
#else
    #error "Unsupported platform"
#endif

/* ============================================================================
 * Platform Headers
 * ============================================================================ */

#ifdef SIGIL_WINDOWS
    #define WIN32_LEAN_AND_MEAN
    #include <windows.h>
#else
    #include <pthread.h>
    #include <unistd.h>
    #include <errno.h>
    #include <time.h>
#endif

/* ============================================================================
 * Thread Types
 * ============================================================================ */

#ifdef SIGIL_WINDOWS
    typedef HANDLE              sigil_thread_t;
    typedef DWORD               sigil_thread_ret_t;
    #define SIGIL_THREAD_CALL   WINAPI
    typedef LPTHREAD_START_ROUTINE sigil_thread_func_t;
#else
    typedef pthread_t           sigil_thread_t;
    typedef void*               sigil_thread_ret_t;
    #define SIGIL_THREAD_CALL
    typedef void* (*sigil_thread_func_t)(void*);
#endif

/* ============================================================================
 * Mutex Types
 * ============================================================================ */

#ifdef SIGIL_WINDOWS
    typedef CRITICAL_SECTION    sigil_mutex_t;
#else
    typedef pthread_mutex_t     sigil_mutex_t;
#endif

/* ============================================================================
 * Condition Variable Types
 * ============================================================================ */

#ifdef SIGIL_WINDOWS
    typedef CONDITION_VARIABLE  sigil_cond_t;
#else
    typedef pthread_cond_t      sigil_cond_t;
#endif

/* ============================================================================
 * Thread Functions
 * ============================================================================ */

static inline int sigil_thread_create(sigil_thread_t* thread, sigil_thread_func_t func, void* arg) {
#ifdef SIGIL_WINDOWS
    *thread = CreateThread(NULL, 0, func, arg, 0, NULL);
    return (*thread != NULL) ? 0 : -1;
#else
    return pthread_create(thread, NULL, func, arg);
#endif
}

static inline int sigil_thread_join(sigil_thread_t thread) {
#ifdef SIGIL_WINDOWS
    DWORD result = WaitForSingleObject(thread, INFINITE);
    CloseHandle(thread);
    return (result == WAIT_OBJECT_0) ? 0 : -1;
#else
    return pthread_join(thread, NULL);
#endif
}

static inline int sigil_thread_join_multiple(sigil_thread_t* threads, uint32_t count) {
#ifdef SIGIL_WINDOWS
    WaitForMultipleObjects(count, threads, TRUE, INFINITE);
    for (uint32_t i = 0; i < count; i++) {
        if (threads[i]) {
            CloseHandle(threads[i]);
        }
    }
    return 0;
#else
    for (uint32_t i = 0; i < count; i++) {
        pthread_join(threads[i], NULL);
    }
    return 0;
#endif
}

/* ============================================================================
 * Mutex Functions
 * ============================================================================ */

static inline int sigil_mutex_init(sigil_mutex_t* mutex) {
#ifdef SIGIL_WINDOWS
    InitializeCriticalSection(mutex);
    return 0;
#else
    return pthread_mutex_init(mutex, NULL);
#endif
}

static inline int sigil_mutex_destroy(sigil_mutex_t* mutex) {
#ifdef SIGIL_WINDOWS
    DeleteCriticalSection(mutex);
    return 0;
#else
    return pthread_mutex_destroy(mutex);
#endif
}

static inline int sigil_mutex_lock(sigil_mutex_t* mutex) {
#ifdef SIGIL_WINDOWS
    EnterCriticalSection(mutex);
    return 0;
#else
    return pthread_mutex_lock(mutex);
#endif
}

static inline int sigil_mutex_unlock(sigil_mutex_t* mutex) {
#ifdef SIGIL_WINDOWS
    LeaveCriticalSection(mutex);
    return 0;
#else
    return pthread_mutex_unlock(mutex);
#endif
}

/* ============================================================================
 * One-time Initialization (race-free lazy init)
 *
 * Use SIGIL_ONCE_INIT to statically initialize a sigil_once_t, then call
 * sigil_call_once(&once, fn) from any thread — fn runs exactly once, with all
 * other callers blocked until it completes. Replaces the racy
 * check-a-flag-then-init pattern.
 * ============================================================================ */

#ifdef SIGIL_WINDOWS
    typedef INIT_ONCE sigil_once_t;
    #define SIGIL_ONCE_INIT INIT_ONCE_STATIC_INIT
    static BOOL CALLBACK sigil__once_adapter(PINIT_ONCE once, PVOID param, PVOID* ctx) {
        (void)once; (void)ctx;
        ((void (*)(void))param)();
        return TRUE;
    }
    static inline void sigil_call_once(sigil_once_t* once, void (*fn)(void)) {
        InitOnceExecuteOnce(once, sigil__once_adapter, (PVOID)(void*)fn, NULL);
    }
#else
    typedef pthread_once_t sigil_once_t;
    #define SIGIL_ONCE_INIT PTHREAD_ONCE_INIT
    static inline void sigil_call_once(sigil_once_t* once, void (*fn)(void)) {
        pthread_once(once, fn);
    }
#endif

/* ============================================================================
 * Condition Variable Functions
 * ============================================================================ */

static inline int sigil_cond_init(sigil_cond_t* cond) {
#ifdef SIGIL_WINDOWS
    InitializeConditionVariable(cond);
    return 0;
#else
    return pthread_cond_init(cond, NULL);
#endif
}

static inline int sigil_cond_destroy(sigil_cond_t* cond) {
#ifdef SIGIL_WINDOWS
    /* Windows condition variables don't need destruction */
    (void)cond;
    return 0;
#else
    return pthread_cond_destroy(cond);
#endif
}

static inline int sigil_cond_wait(sigil_cond_t* cond, sigil_mutex_t* mutex) {
#ifdef SIGIL_WINDOWS
    SleepConditionVariableCS(cond, mutex, INFINITE);
    return 0;
#else
    return pthread_cond_wait(cond, mutex);
#endif
}

static inline int sigil_cond_signal(sigil_cond_t* cond) {
#ifdef SIGIL_WINDOWS
    WakeConditionVariable(cond);
    return 0;
#else
    return pthread_cond_signal(cond);
#endif
}

static inline int sigil_cond_broadcast(sigil_cond_t* cond) {
#ifdef SIGIL_WINDOWS
    WakeAllConditionVariable(cond);
    return 0;
#else
    return pthread_cond_broadcast(cond);
#endif
}

/* ============================================================================
 * System Information
 * ============================================================================ */

static inline uint32_t sigil_get_cpu_count(void) {
#ifdef SIGIL_WINDOWS
    SYSTEM_INFO sys_info;
    GetSystemInfo(&sys_info);
    return (uint32_t)sys_info.dwNumberOfProcessors;
#else
    long count = sysconf(_SC_NPROCESSORS_ONLN);
    return (count > 0) ? (uint32_t)count : 1;
#endif
}

/* ============================================================================
 * Sleep
 * ============================================================================ */

static inline void sigil_sleep_ms(uint32_t milliseconds) {
#ifdef SIGIL_WINDOWS
    Sleep(milliseconds);
#else
    struct timespec ts;
    ts.tv_sec = milliseconds / 1000;
    ts.tv_nsec = (milliseconds % 1000) * 1000000;
    nanosleep(&ts, NULL);
#endif
}

/* ============================================================================
 * Networking
 * ============================================================================ */

#ifdef SIGIL_WINDOWS
    #include <winsock2.h>
    #include <ws2tcpip.h>
    typedef SOCKET sigil_socket_t;
    #define SIGIL_INVALID_SOCKET INVALID_SOCKET
    #define SIGIL_SOCKET_ERROR SOCKET_ERROR
#else
    #include <sys/socket.h>
    #include <netinet/in.h>
    #include <arpa/inet.h>
    #include <netdb.h>
    typedef int sigil_socket_t;
    #define SIGIL_INVALID_SOCKET (-1)
    #define SIGIL_SOCKET_ERROR (-1)
#endif

/* Network initialization (Winsock requires explicit init) */
static inline int sigil_net_platform_init(void) {
#ifdef SIGIL_WINDOWS
    WSADATA wsa_data;
    return WSAStartup(MAKEWORD(2, 2), &wsa_data);
#else
    return 0;  /* No init needed on POSIX */
#endif
}

static inline void sigil_net_platform_shutdown(void) {
#ifdef SIGIL_WINDOWS
    WSACleanup();
#endif
}

static inline int sigil_closesocket(sigil_socket_t sock) {
#ifdef SIGIL_WINDOWS
    return closesocket(sock);
#else
    return close(sock);
#endif
}

#endif /* SIGIL_PLATFORM_H */
