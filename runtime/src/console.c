/*
 * Sigil Runtime - Console Operations
 *
 * Provides cross-platform console I/O for NATIVE behaviors.
 *
 * Cross-platform implementation:
 * - Windows: CRT functions (_write, _read to stdin/stdout)
 * - Linux/macOS: POSIX (write, read to stdin/stdout)
 */

#include "internal.h"
#include <stdio.h>
#include <string.h>

#ifdef SIGIL_WINDOWS
    #include <io.h>
#endif

/* Standard file descriptors */
#define STDIN_FD  0
#define STDOUT_FD 1
#define STDERR_FD 2

/*
 * Print a string to stdout (no newline).
 *
 * text: a Sigil string = [len: 8-byte LE][content]. The length is read from the
 *       prefix; the content follows it. No separate length argument.
 *
 * print is a sink: it consumes a string and emits it, returning nothing. Failure
 * is the concern of the error mechanism, not a returned byte count.
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_print(void* text) {
    int64_t len = *(int64_t*)text;          /* length prefix */
    const void* content = (const char*)text + 8;  /* content follows the prefix */
#ifdef SIGIL_WINDOWS
    int result = _write(STDOUT_FD, content, (unsigned int)len);
#else
    ssize_t result = write(STDOUT_FD, content, (size_t)len);
#endif
    (void)result;
}

/*
 * Print a string to stdout followed by a newline.
 *
 * text: a Sigil string = [len: 8-byte LE][content]. No separate length argument.
 * Returns nothing (a sink). All parameters are pointers per Sigil calling convention.
 */
void sigil_println(void* text) {
    int64_t len = *(int64_t*)text;          /* length prefix */
    const void* content = (const char*)text + 8;  /* content follows the prefix */
    char newline = '\n';
#ifdef SIGIL_WINDOWS
    int r1 = _write(STDOUT_FD, content, (unsigned int)len);
    int r2 = _write(STDOUT_FD, &newline, 1);
#else
    ssize_t r1 = write(STDOUT_FD, content, (size_t)len);
    ssize_t r2 = write(STDOUT_FD, &newline, 1);
#endif
    (void)r1; (void)r2;
}

/*
 * Print raw bytes to stdout with a trailing newline.
 *
 * Unlike sigil_println (which takes a self-describing `string`), this writes an
 * explicit byte count from a `bytes` buffer — for data that is genuinely a raw
 * fixed buffer carrying its length separately (e.g. a value read out of a
 * key/value store's fixed slot), not a length-prefixed string.
 *
 * data: Data buffer (pointer)
 * length: Bytes to write before the newline (pointer to int64)
 *
 * Returns nothing (a sink). All parameters are pointers per Sigil calling convention.
 */
void sigil_println_bytes(void* data, int64_t* length) {
    int64_t len = *length;
    char newline = '\n';
#ifdef SIGIL_WINDOWS
    int r1 = _write(STDOUT_FD, data, (unsigned int)len);
    int r2 = _write(STDOUT_FD, &newline, 1);
#else
    ssize_t r1 = write(STDOUT_FD, data, (size_t)len);
    ssize_t r2 = write(STDOUT_FD, &newline, 1);
#endif
    (void)r1; (void)r2;
}

/*
 * Translate a self-describing string into a raw byte buffer + explicit length.
 *
 * text: a Sigil string = [len: 8-byte LE][content].
 * bytes_out: Output buffer to receive the content (pointer).
 * length_out: Output - number of content bytes copied (pointer to int64).
 *
 * For APIs that operate on raw bytes (e.g. a socket send): bytes are sent over a
 * socket, so a string is translated here first. The output buffer is the Sigil
 * `bytes 65536` slot, so the copy is capped at that capacity.
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_string_to_bytes(void* text, void* bytes_out, int64_t* length_out) {
    int64_t len = *(int64_t*)text;                 /* length prefix */
    const char* content = (const char*)text + 8;   /* content follows the prefix */
    if (len < 0) len = 0;
    /* Leave room for a NUL terminator within the OUTPUT bytes 65536 capacity, so
     * the result is a valid null-terminated buffer for the string ops (find,
     * concat, starts-with, ...). Without this the bytes after the content are
     * uninitialized garbage and a null-scan runs past the real length. */
    if (len > 65535) len = 65535;
    memcpy(bytes_out, content, (size_t)len);
    ((char*)bytes_out)[len] = '\0';
    if (length_out) *length_out = len;
}

/*
 * Read a line from stdin.
 *
 * max_length: Maximum bytes to read (pointer to int64)
 * buffer: Output buffer (pointer)
 * bytes_read: Output - actual bytes read (pointer to int64)
 *
 * Reads until newline (included) or max_length reached.
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_read_line(int64_t* max_length, void* buffer, int64_t* bytes_read) {
    int64_t max_len = *max_length;
    /* The buffer's capacity is its bound. read_line.beh declares OUTPUT buffer
     * bytes 65536, so never read past that regardless of the requested
     * max_length (max_length is a request, the OUTPUT size is the limit). */
    if (max_len > 65536) max_len = 65536;
    if (max_len < 0) max_len = 0;
    char* buf = (char*)buffer;
    int64_t total = 0;
    char c;

    while (total < max_len) {
#ifdef SIGIL_WINDOWS
        int result = _read(STDIN_FD, &c, 1);
#else
        ssize_t result = read(STDIN_FD, &c, 1);
#endif

        if (result < 0) {
            /* Error */
            if (bytes_read) *bytes_read = total;
            return;
        }
        if (result == 0) {
            /* EOF */
            break;
        }

        buf[total++] = c;

        if (c == '\n') {
            /* End of line */
            break;
        }
    }

    if (bytes_read) *bytes_read = total;
}
