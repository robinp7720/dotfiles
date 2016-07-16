#!/usr/bin/env python
from subprocess import call
while True:
    s = str(input())
    if (len(s) > 100):
        s = s[:50] + '...' + s[-50:]
    call(["python3", "/home/robin/.config/i3/themer/__init__.py"])
    print(s)

