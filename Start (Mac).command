#!/bin/bash
# Double-click to start JP's Texas Hold'em on a Mac.
cd "$(dirname "$0")" || exit 1

if command -v python3 >/dev/null 2>&1 && python3 -c "import http.server" >/dev/null 2>&1; then
  exec python3 server.py
elif command -v node >/dev/null 2>&1; then
  exec node server.js
else
  echo "Python 3 or Node.js not found - opening the game directly from the file instead."
  open "index.html"
fi
