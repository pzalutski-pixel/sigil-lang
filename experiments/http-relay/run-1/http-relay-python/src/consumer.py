#!/usr/bin/env python3
"""HTTP Consumer - listens on port 9753 and processes POST requests."""

from http.server import HTTPServer, BaseHTTPRequestHandler
from datetime import datetime

PORT = 9753
OUTPUT_FILE = "output.txt"


class ConsumerHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        content_length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(content_length).decode("utf-8")

        timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
        result = f"{timestamp} PROCESSED BY CONSUMER\n{body}"

        with open(OUTPUT_FILE, "w") as f:
            f.write(result)

        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.end_headers()
        self.wfile.write(b"OK")

        print(f"Processed request, wrote to {OUTPUT_FILE}")

    def log_message(self, format, *args):
        print(f"[{datetime.now().strftime('%H:%M:%S')}] {args[0]}")


def main():
    server = HTTPServer(("", PORT), ConsumerHandler)
    print(f"Consumer listening on port {PORT}...")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down.")
        server.shutdown()


if __name__ == "__main__":
    main()
