#!/bin/sh
# Runs newsqueue-server as PUID:PGID (default 1000:1000) with the TZ time
# zone, and gives a first start the container's folders as defaults.
set -e
PUID="${PUID:-1000}"
PGID="${PGID:-1000}"
if [ -n "$TZ" ] && [ -f "/usr/share/zoneinfo/$TZ" ]; then
    ln -snf "/usr/share/zoneinfo/$TZ" /etc/localtime
    echo "$TZ" > /etc/timezone
fi
mkdir -p /config /downloads /incomplete /watch
if [ ! -f /config/newsqueue-settings.json ]; then
    cat > /config/newsqueue-settings.json <<JSON
{
  "download_dir": "/downloads",
  "temp_dir": "/incomplete",
  "nzb_dir": "/config/nzb",
  "unrar_dir": "",
  "auto_import_folder": "/watch"
}
JSON
fi
if [ "$(id -u)" = "0" ]; then
    chown "$PUID:$PGID" /config /downloads /incomplete /watch /config/newsqueue-settings.json
    exec setpriv --reuid="$PUID" --regid="$PGID" --clear-groups -- "$@"
fi
exec "$@"
