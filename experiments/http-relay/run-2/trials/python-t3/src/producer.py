import os
import urllib.request

INPUT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "input.txt")

with open(INPUT, "rb") as f:
    data = f.read()

req = urllib.request.Request("http://localhost:9812", data=data, method="POST")
with urllib.request.urlopen(req) as resp:
    print(resp.read().decode("utf-8"))
