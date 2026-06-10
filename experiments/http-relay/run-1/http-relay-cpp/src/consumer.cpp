// HTTP Consumer - Listens on port 9753 for POST requests
// Prepends timestamp and "PROCESSED BY CONSUMER" to body, writes to output.txt

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <winsock2.h>
#include <ws2tcpip.h>
#include <stdio.h>
#include <string>
#include <ctime>

#pragma comment(lib, "Ws2_32.lib")

const int PORT = 9753;
const int BUFFER_SIZE = 65536;

std::string get_timestamp() {
    time_t now = time(nullptr);
    struct tm* local_time = localtime(&now);
    char buffer[64];
    strftime(buffer, sizeof(buffer), "%Y-%m-%d %H:%M:%S", local_time);
    return std::string(buffer);
}

std::string extract_body(const char* request, int request_len) {
    // Find the end of headers (double CRLF)
    const char* body_start = strstr(request, "\r\n\r\n");
    if (body_start) {
        body_start += 4; // Skip past \r\n\r\n
        return std::string(body_start);
    }
    return "";
}

bool write_output(const std::string& content) {
    FILE* file = fopen("output.txt", "w");
    if (!file) {
        printf("Error: Could not open output.txt for writing\n");
        return false;
    }
    fwrite(content.c_str(), 1, content.length(), file);
    fclose(file);
    return true;
}

void send_response(SOCKET client_socket, int status_code, const char* status_text) {
    char response[512];
    snprintf(response, sizeof(response),
        "HTTP/1.1 %d %s\r\n"
        "Content-Type: text/plain\r\n"
        "Content-Length: %zu\r\n"
        "Connection: close\r\n"
        "\r\n"
        "%s\n",
        status_code, status_text, strlen(status_text) + 1, status_text);
    send(client_socket, response, (int)strlen(response), 0);
}

int main() {
    WSADATA wsaData;
    SOCKET server_socket = INVALID_SOCKET;
    SOCKET client_socket = INVALID_SOCKET;
    struct sockaddr_in server_addr;
    char buffer[BUFFER_SIZE];

    // Initialize Winsock
    int result = WSAStartup(MAKEWORD(2, 2), &wsaData);
    if (result != 0) {
        printf("WSAStartup failed: %d\n", result);
        return 1;
    }

    // Create socket
    server_socket = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (server_socket == INVALID_SOCKET) {
        printf("Socket creation failed: %d\n", WSAGetLastError());
        WSACleanup();
        return 1;
    }

    // Allow socket reuse
    int opt = 1;
    setsockopt(server_socket, SOL_SOCKET, SO_REUSEADDR, (char*)&opt, sizeof(opt));

    // Setup server address
    server_addr.sin_family = AF_INET;
    server_addr.sin_addr.s_addr = INADDR_ANY;
    server_addr.sin_port = htons(PORT);

    // Bind socket
    result = bind(server_socket, (struct sockaddr*)&server_addr, sizeof(server_addr));
    if (result == SOCKET_ERROR) {
        printf("Bind failed: %d\n", WSAGetLastError());
        closesocket(server_socket);
        WSACleanup();
        return 1;
    }

    // Listen for connections
    result = listen(server_socket, SOMAXCONN);
    if (result == SOCKET_ERROR) {
        printf("Listen failed: %d\n", WSAGetLastError());
        closesocket(server_socket);
        WSACleanup();
        return 1;
    }

    printf("Consumer listening on port %d...\n", PORT);
    printf("Press Ctrl+C to stop\n\n");

    // Main server loop
    while (true) {
        // Accept connection
        client_socket = accept(server_socket, NULL, NULL);
        if (client_socket == INVALID_SOCKET) {
            printf("Accept failed: %d\n", WSAGetLastError());
            continue;
        }

        printf("Connection received\n");

        // Receive data
        memset(buffer, 0, BUFFER_SIZE);
        int bytes_received = recv(client_socket, buffer, BUFFER_SIZE - 1, 0);

        if (bytes_received > 0) {
            buffer[bytes_received] = '\0';

            // Check if it's a POST request
            if (strncmp(buffer, "POST", 4) == 0) {
                std::string body = extract_body(buffer, bytes_received);

                // Create output with timestamp
                std::string output = get_timestamp() + " PROCESSED BY CONSUMER\n" + body;

                if (write_output(output)) {
                    printf("Processed request, wrote to output.txt\n");
                    send_response(client_socket, 200, "OK");
                } else {
                    send_response(client_socket, 500, "Internal Server Error");
                }
            } else {
                printf("Received non-POST request, ignoring\n");
                send_response(client_socket, 405, "Method Not Allowed");
            }
        }

        closesocket(client_socket);
    }

    // Cleanup (unreachable in current loop, but good practice)
    closesocket(server_socket);
    WSACleanup();
    return 0;
}
