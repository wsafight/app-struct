#!/usr/bin/env sh
set -eu

cd /demo

echo "[appstruct-demo] Preparing PostgreSQL schema..."
if [ -d /demo/migrations ] && [ -n "$(ls -A /demo/migrations 2>/dev/null || true)" ]; then
  appstruct --project /demo migrate apply
else
  appstruct --project /demo migrate dev --accept
fi

echo "[appstruct-demo] Starting AppStruct backend on ${APPSTRUCT_BIND:-127.0.0.1:3000}..."
appstruct-generated-server &
API_PID=$!

cleanup() {
  kill "$API_PID" 2>/dev/null || true
}
trap cleanup INT TERM

echo "[appstruct-demo] Starting web frontend on http://0.0.0.0:8080..."
exec nginx -g "daemon off;"
