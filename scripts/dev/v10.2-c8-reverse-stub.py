#!/usr/bin/env python3
"""The host-side service the C8 e2e asks a device to reach.

A file of its own rather than a heredoc: bash 3.2 — what a login shell
finds on macOS — cannot parse a heredoc body containing `$( )` with
parentheses in it, and the C7 script learned that the slow way.

Answers every connection with one known line. The token is passed in
rather than fixed so a stub left over from an earlier run cannot supply
bytes that satisfy this one.
"""

import socket
import sys

port = int(sys.argv[1])
token = sys.argv[2]

server = socket.socket()
server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
server.bind(("127.0.0.1", port))
server.listen(8)
print("listening", flush=True)

while True:
    conn, _ = server.accept()
    try:
        conn.recv(1024)
        body = token.encode()
        conn.sendall(
            b"HTTP/1.0 200 OK\r\nContent-Length: %d\r\n\r\n%s" % (len(body), body)
        )
    finally:
        conn.close()
