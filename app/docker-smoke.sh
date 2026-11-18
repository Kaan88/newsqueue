#!/bin/sh
# Container smoke test (GUI_PLAN Phase 1): build the image, run it with a
# random API key, check health and the egui page, point it at a local mock
# NNTP server, upload an NZB through the API and wait until the history says
# Completed with the right bytes on disk. Needs docker, curl and python3.
#   ./docker-smoke.sh            (run in app/ on a Docker host)
set -eu
cd "$(dirname "$0")"
IMAGE=newsqueue:smoke
NAME=nq-smoke-$$
# Not /tmp: a small tmpfs trips the low-free-space pause.
WORK=$(mktemp -d "$PWD/.smoke.XXXXXX")
PORT=18480
MOCK_PORT=11119
cleanup() {
    docker rm -f "$NAME" >/dev/null 2>&1 || true
    [ -n "${MOCK_PID:-}" ] && kill "$MOCK_PID" 2>/dev/null || true
    rm -rf "$WORK"
}
trap cleanup EXIT
fail() { echo "FAILED: $*"; docker logs "$NAME" 2>&1 | tail -20; exit 1; }

docker build -q -t "$IMAGE" . >/dev/null
KEY=$(head -c 12 /dev/urandom | od -An -tx1 | tr -d ' \n')
python3 tests/docker/mocknntp.py "$MOCK_PORT" &
MOCK_PID=$!
docker run -d --name "$NAME" -p "127.0.0.1:$PORT:8480" -e NQ_API_KEY="$KEY" \
    -e PUID="$(id -u)" -e PGID="$(id -g)" -e TZ=Etc/UTC \
    -v "$WORK/config:/config" -v "$WORK/downloads:/downloads" \
    -v "$WORK/incomplete:/incomplete" -v "$WORK/watch:/watch" "$IMAGE" >/dev/null
api() { curl -s -H "X-Api-Key: $KEY" "$@"; }
for i in $(seq 1 30); do
    [ "$(docker inspect -f '{{.State.Health.Status}}' "$NAME")" = healthy ] && break
    sleep 1
done
[ "$(docker inspect -f '{{.State.Health.Status}}' "$NAME")" = healthy ] || fail "not healthy"
echo "ok: healthy"
curl -s "127.0.0.1:$PORT/" | grep -q newsqueue-web.js || fail "no egui page at /"
echo "ok: egui web UI at /"
[ "$(curl -s -o /dev/null -w '%{http_code}' "127.0.0.1:$PORT/api/state")" = 401 ] || fail "state without key"
echo "ok: API needs the key"
GW=$(docker network inspect bridge -f '{{(index .IPAM.Config 0).Gateway}}')
api -X POST --data "[{\"name\":\"mock\",\"address\":\"$GW\",\"port\":$MOCK_PORT,\"connections\":2,\"ssl\":false,\"auth\":false,\"enabled\":true}]" \
    "127.0.0.1:$PORT/api/servers" | grep -q queued || fail "servers"
cat > "$WORK/smoke.nzb" <<NZB
<?xml version="1.0" encoding="utf-8"?>
<nzb xmlns="http://www.newzbin.com/DTD/2003/nzb">
<file poster="t" date="1763400000" subject="Docker.Smoke [1/1] - &quot;payload.bin&quot; yEnc (1/1)">
<groups><group>alt.binaries.test</group></groups>
<segments><segment bytes="20000" number="1">smoke1@mock</segment></segments>
</file>
</nzb>
NZB
api -X POST --data-binary "@$WORK/smoke.nzb" "127.0.0.1:$PORT/api/nzb?name=Docker.Smoke.nzb" | grep -q queued || fail "upload"
api -X POST "127.0.0.1:$PORT/api/connect" >/dev/null
for i in $(seq 1 60); do
    api "127.0.0.1:$PORT/api/history" | grep -q '"status":"Completed"' && break
    sleep 1
done
api "127.0.0.1:$PORT/api/history" | grep -q '"status":"Completed"' || fail "no Completed history entry"
python3 -c "import sys; sys.exit(open(sys.argv[1],'rb').read() != bytes(i % 256 for i in range(20000)))" \
    "$WORK/downloads/Docker.Smoke/payload.bin" || fail "payload differs"
echo "ok: NZB uploaded, downloaded and in the history"
docker stop "$NAME" >/dev/null
[ -f "$WORK/config/newsqueue-history.json" ] || fail "history not saved on stop"
echo "ok: state saved on docker stop"
echo "DOCKER SMOKE TEST PASSED"
