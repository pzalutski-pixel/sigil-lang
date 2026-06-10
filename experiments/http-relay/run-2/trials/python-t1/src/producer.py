import os
import http.client

INPUT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "input.txt")

with open(INPUT, "r", encoding="utf-8") as f:
    data = f.read()

conn = http.client.HTTPConnection("localhost", 9753)
conn.request("POST", "/", body=data.encode("utf-8"))
resp = conn.getresponse()
resp.read()
conn.close()
print("posted", resp.status)
