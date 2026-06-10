// Consumer: HTTP server on port 9754. On POST, prepend timestamp + marker
// line to the request body and write to output.txt.
#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#include <string>
#include <cstdio>
#include <ctime>

#pragma comment(lib, "ws2_32.lib")

static std::string now_timestamp() {
    SYSTEMTIME st;
    GetLocalTime(&st);
    char buf[32];
    std::snprintf(buf, sizeof(buf), "%04d-%02d-%02d %02d:%02d:%02d",
                  st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond);
    return std::string(buf);
}

// Read entire HTTP request from socket and return the body.
static std::string recv_request_body(SOCKET sock) {
    std::string data;
    char buf[4096];
    int header_end = -1;
    long content_length = -1;

    for (;;) {
        int n = recv(sock, buf, sizeof(buf), 0);
        if (n <= 0) break;
        data.append(buf, n);

        if (header_end < 0) {
            size_t pos = data.find("\r\n\r\n");
            if (pos != std::string::npos) {
                header_end = (int)(pos + 4);
                // parse Content-Length (case-insensitive search on lowercased copy)
                std::string headers = data.substr(0, pos);
                std::string lower = headers;
                for (char& c : lower) c = (char)tolower((unsigned char)c);
                size_t cl = lower.find("content-length:");
                if (cl != std::string::npos) {
                    content_length = std::strtol(headers.c_str() + cl + 15, nullptr, 10);
                }
            }
        }

        if (header_end >= 0) {
            long have = (long)data.size() - header_end;
            if (content_length < 0 || have >= content_length) break;
        }
    }

    if (header_end < 0) return std::string();
    std::string body = data.substr(header_end);
    if (content_length >= 0 && (long)body.size() > content_length) {
        body = body.substr(0, content_length);
    }
    return body;
}

int main() {
    WSADATA wsa;
    if (WSAStartup(MAKEWORD(2, 2), &wsa) != 0) {
        std::fprintf(stderr, "WSAStartup failed\n");
        return 1;
    }

    SOCKET listener = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (listener == INVALID_SOCKET) {
        std::fprintf(stderr, "socket failed: %d\n", WSAGetLastError());
        WSACleanup();
        return 1;
    }

    BOOL opt = TRUE;
    setsockopt(listener, SOL_SOCKET, SO_REUSEADDR, (char*)&opt, sizeof(opt));

    sockaddr_in addr{};
    addr.sin_family = AF_INET;
    addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    addr.sin_port = htons(9754);

    if (bind(listener, (sockaddr*)&addr, sizeof(addr)) == SOCKET_ERROR) {
        std::fprintf(stderr, "bind failed: %d\n", WSAGetLastError());
        closesocket(listener);
        WSACleanup();
        return 1;
    }

    if (listen(listener, SOMAXCONN) == SOCKET_ERROR) {
        std::fprintf(stderr, "listen failed: %d\n", WSAGetLastError());
        closesocket(listener);
        WSACleanup();
        return 1;
    }

    std::fprintf(stderr, "Consumer listening on 9754\n");
    std::fflush(stderr);

    SOCKET client = accept(listener, nullptr, nullptr);
    if (client == INVALID_SOCKET) {
        std::fprintf(stderr, "accept failed: %d\n", WSAGetLastError());
        closesocket(listener);
        WSACleanup();
        return 1;
    }

    std::string body = recv_request_body(client);

    std::string out = now_timestamp() + " PROCESSED BY CONSUMER\n" + body;

    FILE* f = nullptr;
    fopen_s(&f, "output.txt", "wb");
    if (f) {
        std::fwrite(out.data(), 1, out.size(), f);
        std::fclose(f);
    }

    const char* resp = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK";
    send(client, resp, (int)strlen(resp), 0);

    closesocket(client);
    closesocket(listener);
    WSACleanup();
    std::fprintf(stderr, "Consumer wrote output.txt and exiting\n");
    return 0;
}
