#!/usr/bin/env python3
"""HTTP Producer - reads input.txt and sends it to the consumer."""

import urllib.request

URL = "http://localhost:9753"
INPUT_FILE = "input.txt"


def main():
    with open(INPUT_FILE, "r") as f:
        content = f.read()

    data = content.encode("utf-8")
    req = urllib.request.Request(URL, data=data, method="POST")
    req.add_header("Content-Type", "text/plain")

    with urllib.request.urlopen(req) as response:
        print(f"Sent {len(data)} bytes, response: {response.read().decode()}")


if __name__ == "__main__":
    main()
