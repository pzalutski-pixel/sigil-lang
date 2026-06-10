/*
 * Sigil Runtime Library
 *
 * Implements concurrency primitives for the Sigil language.
 * Green thread model: many lightweight tasks mapped to few OS worker threads.
 *
 * Per CONCURRENCY.md: "AI expresses WHAT should run concurrently,
 * runtime figures out HOW to run it efficiently."
 */

#ifndef SIGIL_RUNTIME_H
#define SIGIL_RUNTIME_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ============================================================================
 * Runtime Lifecycle
 * ============================================================================ */

/*
 * Initialize the runtime.
 * Creates worker thread pool, initializes scheduler.
 * Must be called before any other runtime functions.
 */
void sigil_runtime_init(void);

/*
 * Shutdown the runtime.
 * Waits for all tasks to complete, terminates worker threads.
 * Called automatically at program exit if not called explicitly.
 */
void sigil_runtime_shutdown(void);

/* ============================================================================
 * Task (Green Thread) Management
 * ============================================================================ */

/*
 * Spawn a new task (green thread).
 *
 * func: Pointer to the behavior function (compiled Sigil behavior)
 * args: Argument buffer passed to the function
 * args_size: Size of argument buffer in bytes
 *
 * Returns: Task handle (opaque 64-bit identifier)
 *
 * The task executes concurrently on one of the worker threads.
 * Non-blocking: returns immediately after queuing the task.
 */
uint64_t sigil_spawn(void (*func)(void*), void* args, uint64_t args_size);

/*
 * Wait for a task to complete.
 *
 * handle: Task handle from sigil_spawn
 * result: Buffer to receive result (can be NULL)
 * result_size: Size of result buffer
 *
 * Blocks until the task completes.
 * After return, the task handle is invalidated.
 */
void sigil_wait(uint64_t handle, void* result, uint64_t result_size);

/*
 * Wait for all tasks to complete.
 *
 * handles: Array of task handles
 * count: Number of handles in array
 * results: Array of result buffer pointers (can be NULL)
 * result_sizes: Array of result buffer sizes
 *
 * Blocks until all tasks complete.
 * Results are written in same order as handles.
 */
void sigil_wait_all(uint64_t* handles, uint64_t count,
                    void** results, uint64_t* result_sizes);

/*
 * Wait for any task to complete.
 *
 * handles: Array of task handles
 * count: Number of handles in array
 * result: Buffer to receive result from completed task
 * result_size: Size of result buffer
 *
 * Returns: Index of the task that completed (0-based)
 *
 * Blocks until at least one task completes.
 * Only the completed task's handle is invalidated.
 * Other handles remain valid and can be waited on later.
 */
uint64_t sigil_wait_any(uint64_t* handles, uint64_t count,
                        void* result, uint64_t result_size);

/* ============================================================================
 * Channel Communication
 * ============================================================================ */

/*
 * Create a bounded channel.
 *
 * elem_size: Size of each element in bytes
 * capacity: Maximum number of elements (buffer size)
 *
 * Returns: Channel handle (opaque 64-bit identifier)
 *
 * Channels are typed by element size, not by interpretation.
 * Thread-safe: multiple senders and receivers supported.
 */
uint64_t sigil_channel_create(uint64_t elem_size, uint64_t capacity);

/*
 * Send a value to a channel.
 *
 * channel: Channel handle
 * value: Pointer to value to send (elem_size bytes copied)
 *
 * Returns: 0 on success, -1 if channel is closed
 *
 * Blocks if channel is full (at capacity).
 * Unblocks when another task receives.
 */
int64_t sigil_channel_send(uint64_t channel, void* value);

/*
 * Receive a value from a channel.
 *
 * channel: Channel handle
 * value: Buffer to receive value (elem_size bytes)
 *
 * Returns: 0 on success, -1 if channel is closed and empty
 *
 * Blocks if channel is empty.
 * Unblocks when another task sends.
 */
int64_t sigil_channel_receive(uint64_t channel, void* value);

/*
 * Close a channel.
 *
 * channel: Channel handle
 *
 * Signals that no more values will be sent.
 * Senders get -1 after close.
 * Receivers get remaining values, then -1 when empty.
 * Idempotent: closing twice is safe.
 */
void sigil_channel_close(uint64_t channel);

/*
 * Destroy a channel and free resources.
 *
 * channel: Channel handle
 *
 * Should only be called after channel is closed and drained.
 */
void sigil_channel_destroy(uint64_t channel);

/* ============================================================================
 * Console Operations
 * ============================================================================ */

/*
 * Print a string (length-prefixed: [len:8][content]) to stdout (no newline).
 * A sink — returns nothing. All parameters are pointers per Sigil calling convention.
 */
void sigil_print(void* text);

/*
 * Print a string (length-prefixed: [len:8][content]) to stdout with newline.
 * A sink — returns nothing. All parameters are pointers per Sigil calling convention.
 */
void sigil_println(void* text);

/*
 * Print `length` raw bytes from a fixed `bytes` buffer to stdout with newline
 * (length carried separately, not a self-describing string).
 * A sink — returns nothing. All parameters are pointers per Sigil calling convention.
 */
void sigil_println_bytes(void* data, int64_t* length);

/*
 * Translate a self-describing string ([len: 8-byte LE][content]) into a raw byte
 * buffer plus an explicit length, for APIs that take bytes (e.g. socket send).
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_string_to_bytes(void* text, void* bytes_out, int64_t* length_out);

/*
 * Read a line from stdin.
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_read_line(int64_t* max_length, void* buffer, int64_t* bytes_read);

/* ============================================================================
 * File Operations
 * ============================================================================ */

/*
 * Open a file.
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_open(void* path, int64_t* flags, int64_t* fd);

/*
 * Read from file descriptor.
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_read(int64_t* fd, int64_t* count, void* buffer, int64_t* bytes_read);

/*
 * Write to file descriptor.
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_write(int64_t* fd, void* data, int64_t* length, int64_t* bytes_written);

/*
 * Close file descriptor.
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_close(int64_t* fd, int64_t* status);

/*
 * Check if file exists.
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_exists(void* path, int64_t* exists);

/* ============================================================================
 * Time Operations
 * ============================================================================ */

/*
 * Get current time in milliseconds since Unix epoch.
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_now(int64_t* timestamp);

/*
 * Sleep for specified milliseconds.
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_sleep(int64_t* milliseconds, int64_t* status);

/* ============================================================================
 * Network Operations (Winsock2 on Windows)
 * ============================================================================ */

/*
 * Initialize networking subsystem.
 * Called automatically by sigil_runtime_init().
 */
void sigil_net_init(void);

/*
 * Shutdown networking subsystem.
 * Called automatically by sigil_runtime_shutdown().
 */
void sigil_net_shutdown(void);

/*
 * Create a socket.
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_socket(int64_t* domain, int64_t* type, int64_t* fd);

/*
 * Bind socket to address.
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_bind(int64_t* sock, void* addr, int64_t* addrlen, int64_t* status);

/*
 * Listen for incoming connections.
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_listen(int64_t* sock, int64_t* backlog, int64_t* status);

/*
 * Connect to remote address.
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_connect(int64_t* sock, void* addr, int64_t* addrlen, int64_t* status);

/*
 * Accept incoming connection.
 * Parameters match stdlib accept.beh contract order.
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_accept(int64_t* fd, int64_t* client_fd, void* client_addr, int64_t* addr_len);

/*
 * Receive data from socket.
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_receive(int64_t* sock, int64_t* length, void* buffer, int64_t* bytes_received);

/*
 * Send data to socket. `buffer` is a raw byte buffer and `length` the number of
 * bytes to send; a socket sends bytes (to send a string, translate it first).
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_send(int64_t* sock, void* buffer, int64_t* length, int64_t* bytes_sent);

/*
 * Close a socket.
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_close_socket(int64_t* sock, int64_t* status);

/* ============================================================================
 * Internal Types (exposed for debugging/testing)
 * ============================================================================ */

typedef enum {
    TASK_PENDING,      /* Queued but not started */
    TASK_RUNNING,      /* Currently executing on a worker */
    TASK_COMPLETED,    /* Finished successfully */
    TASK_FAILED        /* Finished with error */
} TaskState;

#ifdef __cplusplus
}
#endif

#endif /* SIGIL_RUNTIME_H */
