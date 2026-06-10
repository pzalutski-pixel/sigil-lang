#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#include <string>
#include <fstream>
#include <cstdio>

#pragma comment(lib, "ws2_32.lib")

static std::string timestamp() {
    SYSTEMTIME st;
    GetLocalTime(&st);
    char buf[32];
    sprintf(buf, "%04d-%02d-%02d %02d:%02d:%02d",
            st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond);
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

    int opt = 1;
    setsockopt(listenSock, SOL_SOCKET, SO_REUSEADDR, (const char*)&opt, sizeof(opt));

    sockaddr_in addr;
    addr.sin_family = AF_INET;
    addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    addr.sin_port = htons(9814);

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

    fprintf(stderr, "Consumer listening on 9814\n");
    fflush(stderr);

    SOCKET client = accept(listenSock, NULL, NULL);
    if (client == INVALID_SOCKET) {
        fprintf(stderr, "accept failed\n");
        closesocket(listenSock);
        WSACleanup();
        return 1;
    }

    // Read the full HTTP request
    std::string request;
    char buf[4096];
    int headerEnd = -1;
    int contentLength = -1;

    while (true) {
        int n = recv(client, buf, sizeof(buf), 0);
        if (n <= 0) break;
        request.append(buf, n);

        if (headerEnd < 0) {
            size_t pos = request.find("\r\n\r\n");
            if (pos != std::string::npos) {
                headerEnd = (int)(pos + 4);
                // parse content-length (case-insensitive)
                std::string headers = request.substr(0, pos);
                std::string lower = headers;
                for (char& c : lower) c = (char)tolower((unsigned char)c);
                size_t clpos = lower.find("content-length:");
                if (clpos != std::string::npos) {
                    size_t valStart = clpos + strlen("content-length:");
                    contentLength = atoi(headers.c_str() + valStart);
                }
            }
        }

        if (headerEnd >= 0) {
            int bodyHave = (int)request.size() - headerEnd;
            if (contentLength < 0 || bodyHave >= contentLength) break;
        }
    }

    std::string body;
    if (headerEnd >= 0) {
        body = request.substr(headerEnd);
        if (contentLength >= 0 && (int)body.size() > contentLength) {
            body = body.substr(0, contentLength);
        }
    }

    std::string result = timestamp() + " PROCESSED BY CONSUMER\n" + body;

    std::ofstream out("output.txt", std::ios::binary);
    out.write(result.data(), (std::streamsize)result.size());
    out.close();

    const char* resp =
        "HTTP/1.1 200 OK\r\n"
        "Content-Length: 2\r\n"
        "Connection: close\r\n"
        "\r\n"
        "OK";
    send(client, resp, (int)strlen(resp), 0);

    closesocket(client);
    closesocket(listenSock);
    WSACleanup();
    return 0;
}
