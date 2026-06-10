import os
import http.client

INPUT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "input.txt")

with open(INPUT, "r", encoding="utf-8") as f:
    body = f.read()

conn = http.client.HTTPConnection("localhost", 9811)
conn.request("POST", "/", body)
resp = conn.getresponse()
print(resp.status, resp.read().decode())
conn.close()
