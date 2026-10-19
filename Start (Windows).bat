@echo off
REM Double-click to start JP's Texas Hold'em on Windows.
cd /d "%~dp0"
title JP's Texas Hold'em

REM 1) Python launcher (python.org installs)
py -3 -c "import http.server" >nul 2>nul && (py -3 server.py & goto :eof)

REM 2) python on PATH (skips the Microsoft Store placeholder, which fails this check)
python -c "import http.server" >nul 2>nul && (python server.py & goto :eof)

REM 3) Node.js
node -v >nul 2>nul && (node server.js & goto :eof)

REM 4) Nothing installed: the game still runs straight from the file.
echo Python 3 or Node.js not found - opening the game directly from the file instead.
start "" "%~dp0index.html"
