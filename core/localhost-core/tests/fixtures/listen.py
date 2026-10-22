"""Listens on a TCP port until stopped, closing every connection it accepts:
the Windows stand-in for `nc -lk` in the port tests.

    python listen.py PORT [--after SECONDS] [--ipv6-loopback]
"""

import argparse
import socket
import time

parser = argparse.ArgumentParser()
parser.add_argument("port", type=int)
parser.add_argument("--after", type=float, default=0, help="wait this long before listening")
parser.add_argument("--ipv6-loopback", action="store_true", help="listen on ::1 only")
arguments = parser.parse_args()

time.sleep(arguments.after)
if arguments.ipv6_loopback:
    server = socket.socket(socket.AF_INET6, socket.SOCK_STREAM)
    server.bind(("::1", arguments.port))
else:
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.bind(("127.0.0.1", arguments.port))
server.listen()
while True:
    connection, _ = server.accept()
    connection.close()
