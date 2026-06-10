/*
 * Sigil Runtime - Network Operations
 *
 * Per the language reference Section 4.10 and the concurrency spec:
 * Provides network primitives for socket communication.
 *
 * Cross-platform implementation:
 * - Windows: Winsock2
 * - Linux/macOS: BSD sockets
 *
 * API matches Linux syscall semantics for stdlib compatibility.
 */

#include "internal.h"

/* Network initialization state */
static volatile int g_net_initialized = 0;
static sigil_mutex_t g_net_lock;
static sigil_once_t g_net_lock_once = SIGIL_ONCE_INIT;

static void init_net_lock(void) { sigil_mutex_init(&g_net_lock); }

/*
 * Initialize networking subsystem.
 * Thread-safe - can be called multiple times.
 */
void sigil_net_init(void) {
    /* Race-free one-time lock init (was a check-a-flag-then-init TOCTOU). */
    sigil_call_once(&g_net_lock_once, init_net_lock);

    sigil_mutex_lock(&g_net_lock);

    if (!g_net_initialized) {
        int result = sigil_net_platform_init();
        if (result == 0) {
            g_net_initialized = 1;
        }
    }

    sigil_mutex_unlock(&g_net_lock);
}

/*
 * Shutdown networking subsystem.
 * Thread-safe.
 */
void sigil_net_shutdown(void) {
    /* Ensure the lock exists before touching shared state (no-op if init ran). */
    sigil_call_once(&g_net_lock_once, init_net_lock);

    sigil_mutex_lock(&g_net_lock);

    if (g_net_initialized) {
        sigil_net_platform_shutdown();
        g_net_initialized = 0;
    }

    sigil_mutex_unlock(&g_net_lock);
}

/*
 * Create a socket.
 *
 * domain: Address family - 2=AF_INET for IPv4 (pointer to int64)
 * type: Socket type - 1=SOCK_STREAM for TCP (pointer to int64)
 * fd: Output - socket handle, -1 on error (pointer to int64)
 *
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_socket(int64_t* domain, int64_t* type, int64_t* fd) {
    /* Ensure networking is initialized */
    sigil_net_init();

    int64_t d = *domain;
    int64_t t = *type;
    sigil_socket_t sock = socket((int)d, (int)t, 0);

    if (fd) {
        *fd = (sock == SIGIL_INVALID_SOCKET) ? -1 : (int64_t)sock;
    }
}

/*
 * Bind socket to address.
 *
 * sock: Socket handle (pointer to int64)
 * addr: Pointer to sockaddr structure (pointer)
 * addrlen: Size of address structure (pointer to int64)
 * status: Output - 0 on success, -1 on error (pointer to int64)
 *
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_bind(int64_t* sock, void* addr, int64_t* addrlen, int64_t* status) {
    int64_t s = *sock;
    int64_t len = *addrlen;
    int result = bind((sigil_socket_t)s, (struct sockaddr*)addr, (int)len);

    if (status) *status = (result == 0) ? 0 : -1;
}

/*
 * Listen for incoming connections.
 *
 * sock: Socket handle (pointer to int64)
 * backlog: Maximum pending connections (pointer to int64)
 * status: Output - 0 on success, -1 on error (pointer to int64)
 *
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_listen(int64_t* sock, int64_t* backlog, int64_t* status) {
    int64_t s = *sock;
    int64_t b = *backlog;
    int result = listen((sigil_socket_t)s, (int)b);

    if (status) *status = (result == 0) ? 0 : -1;
}

/*
 * Connect to remote address.
 *
 * sock: Socket handle (pointer to int64)
 * addr: Pointer to sockaddr structure (pointer)
 * addrlen: Size of address structure (pointer to int64)
 * status: Output - 0 on success, -1 on error (pointer to int64)
 *
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_connect(int64_t* sock, void* addr, int64_t* addrlen, int64_t* status) {
    int64_t s = *sock;
    int64_t len = *addrlen;
    int result = connect((sigil_socket_t)s, (struct sockaddr*)addr, (int)len);

    if (status) *status = (result == 0) ? 0 : -1;
}

/*
 * Accept incoming connection.
 *
 * Parameters match stdlib accept.beh contract order (inputs, then outputs):
 *   INPUT fd int 8
 *   OUTPUT client_fd int 8
 *   OUTPUT client_addr bytes 128
 *   OUTPUT addr_len int 8
 *
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_accept(int64_t* fd, int64_t* client_fd, void* client_addr, int64_t* addr_len) {
    int64_t s = *fd;
#ifdef SIGIL_WINDOWS
    int len = sizeof(struct sockaddr_in);
    sigil_socket_t client_sock = accept((sigil_socket_t)s, (struct sockaddr*)client_addr, client_addr ? &len : NULL);
    if (addr_len) {
        *addr_len = (int64_t)len;
    }
#else
    socklen_t len = sizeof(struct sockaddr_in);
    sigil_socket_t client_sock = accept((sigil_socket_t)s, (struct sockaddr*)client_addr, client_addr ? &len : NULL);
    if (addr_len) {
        *addr_len = (int64_t)len;
    }
#endif

    if (client_fd) {
        *client_fd = (client_sock == SIGIL_INVALID_SOCKET) ? -1 : (int64_t)client_sock;
    }
}

/*
 * Receive data from socket.
 *
 * sock: Socket handle (pointer to int64)
 * length: Maximum bytes to receive (pointer to int64)
 * buffer: Buffer to receive data (pointer)
 * bytes_received: Output - bytes received, 0 on closed, -1 on error (pointer to int64)
 *
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_receive(int64_t* sock, int64_t* length, void* buffer, int64_t* bytes_received) {
    int64_t s = *sock;
    int64_t len = *length;
    /* The buffer's capacity is its bound. receive.beh declares OUTPUT buffer
     * bytes 65536, so the runtime must not write past that regardless of the
     * requested length (length is a request, the OUTPUT size is the limit). */
    if (len > 65536) len = 65536;
    if (len < 0) len = 0;
#ifdef SIGIL_WINDOWS
    int result = recv((sigil_socket_t)s, (char*)buffer, (int)len, 0);
    if (result == SIGIL_SOCKET_ERROR) {
        if (bytes_received) *bytes_received = -1;
        return;
    }
#else
    ssize_t result = recv((sigil_socket_t)s, buffer, (size_t)len, 0);
    if (result < 0) {
        if (bytes_received) *bytes_received = -1;
        return;
    }
#endif

    if (bytes_received) *bytes_received = (int64_t)result;
}

/*
 * Send data to socket.
 *
 * sock: Socket handle (pointer to int64)
 * buffer: raw byte buffer to send (pointer)
 * length: number of bytes to send (pointer to int64)
 * bytes_sent: Output - bytes sent, -1 on error (pointer to int64)
 *
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_send(int64_t* sock, void* buffer, int64_t* length, int64_t* bytes_sent) {
    int64_t s = *sock;
    int64_t len = *length;
    const char* content = (const char*)buffer;
    if (len < 0) len = 0;
#ifdef SIGIL_WINDOWS
    int result = send((sigil_socket_t)s, content, (int)len, 0);
    if (result == SIGIL_SOCKET_ERROR) {
        if (bytes_sent) *bytes_sent = -1;
        return;
    }
#else
    ssize_t result = send((sigil_socket_t)s, content, (size_t)len, 0);
    if (result < 0) {
        if (bytes_sent) *bytes_sent = -1;
        return;
    }
#endif

    if (bytes_sent) *bytes_sent = (int64_t)result;
}

/*
 * Close a socket.
 *
 * sock: Socket handle (pointer to int64)
 * status: Output - 0 on success, -1 on error (pointer to int64)
 *
 * All parameters are pointers per Sigil calling convention.
 */
void sigil_close_socket(int64_t* sock, int64_t* status) {
    int64_t s = *sock;
    int result = sigil_closesocket((sigil_socket_t)s);

    if (status) *status = (result == 0) ? 0 : -1;
}
