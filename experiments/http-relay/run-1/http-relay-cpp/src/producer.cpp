// HTTP Producer - Reads input.txt and sends via HTTP POST to localhost:9753

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <winsock2.h>
#include <ws2tcpip.h>
#include <stdio.h>
#include <string>

#pragma comment(lib, "Ws2_32.lib")

const char* HOST = "127.0.0.1";
const int PORT = 9753;
const int BUFFER_SIZE = 65536;

std::string read_input_file() {
    FILE* file = fopen("input.txt", "rb");
    if (!file) {
        printf("Error: Could not open input.txt\n");
        return "";
    }

    // Get file size
    fseek(file, 0, SEEK_END);
    long size = ftell(file);
    fseek(file, 0, SEEK_SET);

    // Read content
    std::string content;
    content.resize(size);
    fread(&content[0], 1, size, file);
    fclose(file);

    return content;
}

int main() {
    WSADATA wsaData;
    SOCKET client_socket = INVALID_SOCKET;
    struct sockaddr_in server_addr;
    char recv_buffer[BUFFER_SIZE];

    // Read input file
    std::string content = read_input_file();
    if (content.empty()) {
        printf("No content to send or file not found\n");
        return 1;
    }

    printf("Read %zu bytes from input.txt\n", content.length());

    // Initialize Winsock
    int result = WSAStartup(MAKEWORD(2, 2), &wsaData);
    if (result != 0) {
        printf("WSAStartup failed: %d\n", result);
        return 1;
    }

    // Create socket
    client_socket = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (client_socket == INVALID_SOCKET) {
        printf("Socket creation failed: %d\n", WSAGetLastError());
        WSACleanup();
        return 1;
    }

    // Setup server address
    server_addr.sin_family = AF_INET;
    server_addr.sin_port = htons(PORT);
    inet_pton(AF_INET, HOST, &server_addr.sin_addr);

    // Connect to server
    result = connect(client_socket, (struct sockaddr*)&server_addr, sizeof(server_addr));
    if (result == SOCKET_ERROR) {
        printf("Connection failed: %d\n", WSAGetLastError());
        printf("Make sure consumer.exe is running on port %d\n", PORT);
        closesocket(client_socket);
        WSACleanup();
        return 1;
    }

    printf("Connected to consumer on port %d\n", PORT);

    // Build HTTP POST request
    std::string request =
        "POST / HTTP/1.1\r\n"
        "Host: localhost:" + std::to_string(PORT) + "\r\n"
        "Content-Type: text/plain\r\n"
        "Content-Length: " + std::to_string(content.length()) + "\r\n"
        "Connection: close\r\n"
        "\r\n" + content;

    // Send request
    result = send(client_socket, request.c_str(), (int)request.length(), 0);
    if (result == SOCKET_ERROR) {
        printf("Send failed: %d\n", WSAGetLastError());
        closesocket(client_socket);
        WSACleanup();
        return 1;
    }

    printf("Sent %d bytes\n", result);

    // Receive response
    memset(recv_buffer, 0, BUFFER_SIZE);
    int bytes_received = recv(client_socket, recv_buffer, BUFFER_SIZE - 1, 0);
    if (bytes_received > 0) {
        recv_buffer[bytes_received] = '\0';

        // Check for HTTP 200 OK
        if (strstr(recv_buffer, "200 OK")) {
            printf("Server responded: OK\n");
        } else {
            printf("Server response:\n%s\n", recv_buffer);
        }
    }

    // Cleanup
    closesocket(client_socket);
    WSACleanup();

    printf("Done!\n");
    return 0;
}
