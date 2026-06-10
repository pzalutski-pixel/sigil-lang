#include <winsock2.h>
#include <ws2tcpip.h>
#include <string>
#include <fstream>
#include <sstream>
#include <cstdio>

#pragma comment(lib, "ws2_32.lib")

int main() {
    std::ifstream in("input.txt", std::ios::binary);
    if (!in) {
        fprintf(stderr, "cannot open input.txt\n");
        return 1;
    }
    std::stringstream ss;
    ss << in.rdbuf();
    std::string body = ss.str();
    in.close();

    WSADATA wsa;
    if (WSAStartup(MAKEWORD(2, 2), &wsa) != 0) {
        fprintf(stderr, "WSAStartup failed\n");
        return 1;
    }

    SOCKET sock = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (sock == INVALID_SOCKET) {
        fprintf(stderr, "socket failed\n");
        WSACleanup();
        return 1;
    }

    sockaddr_in addr{};
    addr.sin_family = AF_INET;
    addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    addr.sin_port = htons(9813);

    if (connect(sock, (sockaddr*)&addr, sizeof(addr)) == SOCKET_ERROR) {
        fprintf(stderr, "connect failed: %d\n", WSAGetLastError());
        closesocket(sock);
        WSACleanup();
        return 1;
    }

    std::ostringstream req;
    req << "POST / HTTP/1.1\r\n"
        << "Host: localhost:9813\r\n"
        << "Content-Type: text/plain\r\n"
        << "Content-Length: " << body.size() << "\r\n"
        << "Connection: close\r\n"
        << "\r\n"
        << body;
    std::string reqStr = req.str();

    int sent = send(sock, reqStr.c_str(), (int)reqStr.size(), 0);
    if (sent == SOCKET_ERROR) {
        fprintf(stderr, "send failed\n");
        closesocket(sock);
        WSACleanup();
        return 1;
    }

    // read response (optional)
    char buf[1024];
    while (recv(sock, buf, sizeof(buf), 0) > 0) {}

    closesocket(sock);
    WSACleanup();
    printf("Producer sent %d bytes\n", sent);
    return 0;
}
