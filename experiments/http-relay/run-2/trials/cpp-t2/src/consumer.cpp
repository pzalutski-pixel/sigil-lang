#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#include <string>
#include <fstream>
#include <ctime>
#include <cstdio>

#pragma comment(lib, "ws2_32.lib")

static std::string timestamp() {
    time_t t = time(nullptr);
    struct tm lt;
    localtime_s(&lt, &t);
    char buf[32];
    strftime(buf, sizeof(buf), "%Y-%m-%d %H:%M:%S", &lt);
    return std::string(buf);
}

int main() {
    WSADATA wsa;
    if (WSAStartup(MAKEWORD(2, 2), &wsa) != 0) {
        fprintf(stderr, "WSAStartup failed\n");
        return 1;
    }

    SOCKET listenSock = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (listenSock == INVALID_SOCKET) {
        fprintf(stderr, "socket failed\n");
        WSACleanup();
        return 1;
    }

    BOOL opt = TRUE;
    setsockopt(listenSock, SOL_SOCKET, SO_REUSEADDR, (char*)&opt, sizeof(opt));

    sockaddr_in addr{};
    addr.sin_family = AF_INET;
    addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    addr.sin_port = htons(9813);

    if (bind(listenSock, (sockaddr*)&addr, sizeof(addr)) == SOCKET_ERROR) {
        fprintf(stderr, "bind failed: %d\n", WSAGetLastError());
        closesocket(listenSock);
        WSACleanup();
        return 1;
    }

    if (listen(listenSock, 1) == SOCKET_ERROR) {
        fprintf(stderr, "listen failed\n");
        closesocket(listenSock);
        WSACleanup();
        return 1;
    }

    printf("Consumer listening on 9813\n");
    fflush(stdout);

    SOCKET client = accept(listenSock, nullptr, nullptr);
    if (client == INVALID_SOCKET) {
        fprintf(stderr, "accept failed\n");
        closesocket(listenSock);
        WSACleanup();
        return 1;
    }

    // Read full HTTP request
    std::string request;
    char buf[4096];
    int contentLength = -1;
    size_t headerEnd = std::string::npos;

    while (true) {
        int n = recv(client, buf, sizeof(buf), 0);
        if (n <= 0) break;
        request.append(buf, n);

        if (headerEnd == std::string::npos) {
            headerEnd = request.find("\r\n\r\n");
            if (headerEnd != std::string::npos) {
                // parse Content-Length (case-insensitive)
                std::string lower = request.substr(0, headerEnd);
                for (auto& c : lower) c = (char)tolower((unsigned char)c);
                size_t pos = lower.find("content-length:");
                if (pos != std::string::npos) {
                    contentLength = atoi(request.c_str() + pos + 15);
                }
            }
        }

        if (headerEnd != std::string::npos) {
            size_t bodyStart = headerEnd + 4;
            size_t haveBody = request.size() - bodyStart;
            if (contentLength < 0 || (int)haveBody >= contentLength) break;
        }
    }

    std::string body;
    if (headerEnd != std::string::npos) {
        size_t bodyStart = headerEnd + 4;
        if (contentLength >= 0) {
            body = request.substr(bodyStart, contentLength);
        } else {
            body = request.substr(bodyStart);
        }
    }

    std::string result = timestamp() + " PROCESSED BY CONSUMER\n" + body + "\n";

    std::ofstream out("output.txt", std::ios::binary);
    out << result;
    out.close();

    const char* resp = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK";
    send(client, resp, (int)strlen(resp), 0);

    closesocket(client);
    closesocket(listenSock);
    WSACleanup();

    printf("Consumer wrote output.txt\n");
    return 0;
}
