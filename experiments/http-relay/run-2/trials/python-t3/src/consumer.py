import os
from datetime import datetime
from http.server import BaseHTTPRequestHandler, HTTPServer

OUTPUT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "output.txt")


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(length).decode("utf-8")
        timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
        result = f"{timestamp} PROCESSED BY CONSUMER\n{body}"
        with open(OUTPUT, "w", encoding="utf-8", newline="") as f:
            f.write(result)
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"OK")

    def log_message(self, *args):
        pass


if __name__ == "__main__":
    HTTPServer(("localhost", 9812), Handler).serve_forever()
