// Producer: read input.txt and POST its contents to localhost:9754.
#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#include <string>
#include <cstdio>

#pragma comment(lib, "ws2_32.lib")

static std::string read_file(const char* path) {
    FILE* f = nullptr;
    fopen_s(&f, path, "rb");
    if (!f) return std::string();
    std::string data;
    char buf[4096];
    size_t n;
    while ((n = fread(buf, 1, sizeof(buf), f)) > 0) data.append(buf, n);
    fclose(f);
    return data;
}

int main() {
    WSADATA wsa;
    if (WSAStartup(MAKEWORD(2, 2), &wsa) != 0) {
        std::fprintf(stderr, "WSAStartup failed\n");
        return 1;
    }

    std::string body = read_file("input.txt");

    SOCKET sock = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (sock == INVALID_SOCKET) {
        std::fprintf(stderr, "socket failed: %d\n", WSAGetLastError());
        WSACleanup();
        return 1;
    }

    sockaddr_in addr{};
    addr.sin_family = AF_INET;
    addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    addr.sin_port = htons(9754);

    if (connect(sock, (sockaddr*)&addr, sizeof(addr)) == SOCKET_ERROR) {
        std::fprintf(stderr, "connect failed: %d\n", WSAGetLastError());
        closesocket(sock);
        WSACleanup();
        return 1;
    }

    std::string req =
        "POST / HTTP/1.1\r\n"
        "Host: localhost:9754\r\n"
        "Content-Type: text/plain\r\n"
        "Content-Length: " + std::to_string(body.size()) + "\r\n"
        "Connection: close\r\n"
        "\r\n" + body;

    int sent = 0;
    int total = (int)req.size();
    while (sent < total) {
        int n = send(sock, req.data() + sent, total - sent, 0);
        if (n == SOCKET_ERROR) {
            std::fprintf(stderr, "send failed: %d\n", WSAGetLastError());
            closesocket(sock);
            WSACleanup();
            return 1;
        }
        sent += n;
    }

    // read response (optional)
    char buf[1024];
    while (recv(sock, buf, sizeof(buf), 0) > 0) {}

    closesocket(sock);
    WSACleanup();
    std::fprintf(stderr, "Producer sent %d bytes\n", total);
    return 0;
}
