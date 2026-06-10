/*
 * Sigil Runtime - File Operations
 *
 * Provides cross-platform file I/O for NATIVE behaviors.
 *
 * Cross-platform implementation:
 * - Windows: CRT functions (_open, _read, _write, _close, _stat)
 * - Linux/macOS: POSIX (open, read, write, close, stat)
 */

#include "internal.h"
#include <stdio.h>
#include <string.h>
#include <stdlib.h>

#ifdef SIGIL_WINDOWS
    #include <io.h>
    #include <fcntl.h>
    #include <sys/stat.h>
    #include <sys/types.h>
    #define O_RDONLY _O_RDONLY
    #define O_WRONLY _O_WRONLY
    #define O_RDWR   _O_RDWR
    #define O_CREAT  _O_CREAT
    #define O_TRUNC  _O_TRUNC
    #define O_APPEND _O_APPEND
#else
    #include <fcntl.h>
    #include <sys/stat.h>
    #include <sys/types.h>
#endif

/*
 * Guard against the UCRT fast-failing the whole process on a bad descriptor.
 *
 * On Windows, _read/_write/_close validate the file descriptor and, on an
 * invalid one, invoke the invalid-parameter handler. The default handler does
 * NOT return an error — it terminates the process via __fastfail with exit code
 * 0xC0000409 (STATUS_STACK_BUFFER_OVERRUN). A normal Sigil program triggers this
 * the moment a file is missing: `open` returns fd = -1, and the next `read`/
 * `close` on that fd crashes the whole program. (This was the intermittent
 * "0xC0000409 in the socket path" — it is actually the file path: it only
 * surfaces when open() fails, e.g. a missing input file or wrong CWD.)
 *
 * Two layers of defense: install a silent invalid-parameter handler so the CRT
 * returns its normal error (-1/errno), AND short-circuit a negative fd before it
 * ever reaches the CRT so the behavior reports failure through its status/zero
 * output like any other I/O error.
 */
#ifdef SIGIL_WINDOWS
static void sigil_silent_invalid_parameter(
        const wchar_t* expr, const wchar_t* func, const wchar_t* file,
        unsigned int line, uintptr_t reserved) {
    (void)expr; (void)func; (void)file; (void)line; (void)reserved;
}
static sigil_once_t g_file_guard_once = SIGIL_ONCE_INIT;
static void install_file_guard(void) {
    _set_invalid_parameter_handler(sigil_silent_invalid_parameter);
}
static void sigil_file_guard(void) { sigil_call_once(&g_file_guard_once, install_file_guard); }
#else
static void sigil_file_guard(void) {}
#endif

/*
 * Open a file.
 *
 * path: a Sigil `string` = [len: 8-byte LE][content]. A path is variable-length
 *       text, so it is a string, not a fixed `bytes` buffer. The OS open needs a
 *       null-terminated C string, so the content is copied out and terminated.
 * flags: Sigil flags - 0=read, 1=write, 2=create, 4=append (pointer to int64)
 * fd: Output - file descriptor (pointer to int64)
 *
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_open(void* path, int64_t* flags, int64_t* fd) {
    sigil_file_guard();
    int64_t f = *flags;
    int os_flags = 0;
    int mode = 0644;  /* Default mode for created files */

    /* Copy the string's content into a null-terminated buffer for the OS call. */
    int64_t path_len = *(int64_t*)path;
    const char* path_content = (const char*)path + 8;
    char path_buf[4096];
    if (path_len < 0) path_len = 0;
    if (path_len > (int64_t)sizeof(path_buf) - 1) path_len = (int64_t)sizeof(path_buf) - 1;
    memcpy(path_buf, path_content, (size_t)path_len);
    path_buf[path_len] = '\0';

    /* Convert Sigil flags to OS flags */
    if (f == 0) {
        /* Read only */
        os_flags = O_RDONLY;
    } else {
        /* Check for write flag */
        if (f & 1) {
            os_flags = O_WRONLY;
        }
        /* Check for create flag */
        if (f & 2) {
            os_flags |= O_CREAT | O_TRUNC;
        }
        /* Check for append flag */
        if (f & 4) {
            os_flags |= O_APPEND;
            os_flags &= ~O_TRUNC;  /* Don't truncate when appending */
        }
    }

#ifdef SIGIL_WINDOWS
    int result = _open(path_buf, os_flags | _O_BINARY, _S_IREAD | _S_IWRITE);
#else
    int result = open(path_buf, os_flags, mode);
#endif

    if (fd) *fd = (int64_t)result;
}

/*
 * Read from file descriptor.
 *
 * fd: File descriptor (pointer to int64)
 * count: Maximum bytes to read (pointer to int64)
 * buffer: Output buffer (pointer)
 * bytes_read: Output - actual bytes read (pointer to int64)
 *
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_read(int64_t* fd, int64_t* count, void* buffer, int64_t* bytes_read) {
    sigil_file_guard();
    int64_t f = *fd;
    int64_t c = *count;
    /* A bad fd (e.g. -1 from a failed open) must not reach the CRT, which would
     * fast-fail the process instead of returning an error. Report 0 bytes. */
    if (f < 0) {
        if (bytes_read) *bytes_read = 0;
        return;
    }
    /* The buffer's capacity is its bound. read.beh declares OUTPUT buffer
     * bytes 65536, so the runtime must not write past that regardless of the
     * requested count (count is a request, the OUTPUT size is the limit).
     * Clamp here, or a count > 65536 overruns the buffer (0xC0000409). */
    if (c > 65536) c = 65536;
    if (c < 0) c = 0;
#ifdef SIGIL_WINDOWS
    int result = _read((int)f, buffer, (unsigned int)c);
#else
    ssize_t result = read((int)f, buffer, (size_t)c);
#endif

    if (result < 0) {
        if (bytes_read) *bytes_read = 0;
        return;
    }

    if (bytes_read) *bytes_read = (int64_t)result;
}

/*
 * Write to file descriptor.
 *
 * fd: File descriptor (pointer to int64)
 * data: Data buffer (pointer)
 * length: Bytes to write (pointer to int64)
 * bytes_written: Output - actual bytes written (pointer to int64)
 *
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_write(int64_t* fd, void* data, int64_t* length, int64_t* bytes_written) {
    sigil_file_guard();
    int64_t f = *fd;
    int64_t len = *length;
    if (f < 0) {
        if (bytes_written) *bytes_written = 0;
        return;
    }
#ifdef SIGIL_WINDOWS
    int result = _write((int)f, data, (unsigned int)len);
#else
    ssize_t result = write((int)f, data, (size_t)len);
#endif

    if (result < 0) {
        if (bytes_written) *bytes_written = 0;
        return;
    }

    if (bytes_written) *bytes_written = (int64_t)result;
}

/*
 * Close file descriptor.
 *
 * fd: File descriptor (pointer to int64)
 * status: Output - 0 on success, -1 on error (pointer to int64)
 *
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_close(int64_t* fd, int64_t* status) {
    sigil_file_guard();
    int64_t f = *fd;
    if (f < 0) {
        if (status) *status = -1;
        return;
    }
#ifdef SIGIL_WINDOWS
    int result = _close((int)f);
#else
    int result = close((int)f);
#endif

    if (status) *status = (result == 0) ? 0 : -1;
}

/*
 * Check if file exists.
 *
 * path: a Sigil string ([len: 8-byte LE][content]) — same path ABI as sigil_open.
 * exists: Output - 1 if exists, 0 if not (pointer to int64)
 *
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_exists(void* path, int64_t* exists) {
    /* Copy the string's content into a null-terminated buffer for the OS call. */
    int64_t path_len = *(int64_t*)path;
    const char* path_content = (const char*)path + 8;
    char path_buf[4096];
    if (path_len < 0) path_len = 0;
    if (path_len > (int64_t)sizeof(path_buf) - 1) path_len = (int64_t)sizeof(path_buf) - 1;
    memcpy(path_buf, path_content, (size_t)path_len);
    path_buf[path_len] = '\0';

#ifdef SIGIL_WINDOWS
    struct _stat buf;
    int result = _stat(path_buf, &buf);
#else
    struct stat buf;
    int result = stat(path_buf, &buf);
#endif

    if (exists) *exists = (result == 0) ? 1 : 0;
}
