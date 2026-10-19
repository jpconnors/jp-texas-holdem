#!/usr/bin/env python3
"""Serve JP's Texas Hold'em locally and open it in the default browser.

Usage:  python3 server.py [port]      (default port 8765; picks the next free one if taken)
Standard library only - no installs needed.
"""
import functools
import http.server
import os
import socket
import sys
import threading
import webbrowser

ROOT = os.path.dirname(os.path.abspath(__file__))


def free_port(start):
    for port in range(start, start + 50):
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            try:
                s.bind(("127.0.0.1", port))
                return port
            except OSError:
                continue
    raise SystemExit("No free port found near %d" % start)


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, ".js": "text/javascript"}

    def log_message(self, *args):
        pass

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()


def main():
    port = free_port(int(sys.argv[1]) if len(sys.argv) > 1 else 8765)
    handler = functools.partial(QuietHandler, directory=ROOT)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", port), handler)
    url = "http://127.0.0.1:%d/" % port
    print("JP's Texas Hold'em is running at %s" % url)
    print("Close this window (or press Ctrl+C) to stop the server.")
    threading.Timer(0.6, lambda: webbrowser.open(url)).start()
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
