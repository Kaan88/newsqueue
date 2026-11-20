# NewsQueue with Docker Compose

Release **1.0.9** includes matching Windows, server and web builds. The prebuilt
container targets **Linux amd64** (x86-64 Linux hosts, or Docker Desktop using
Linux containers).

## Start with the prebuilt image

Install Docker Engine with the Compose plugin, or Docker Desktop. From the
[1.0.9 release](https://github.com/Kaan88/newsqueue/releases/tag/v1.0.9), download
these files into a new directory:

- `NewsQueue-1.0.9-linux-amd64.tar.gz`
- `compose.yaml`
- `env.example`
- `NewsQueue-1.0.9-SHA256SUMS.txt`

On Linux/macOS, copy the environment template and generate a key:

```sh
cp env.example .env
openssl rand -hex 32
```

On Windows PowerShell, use `Copy-Item env.example .env`; generate a key with:

```powershell
$bytes = New-Object byte[] 32
$rng = [Security.Cryptography.RandomNumberGenerator]::Create()
$rng.GetBytes($bytes)
([BitConverter]::ToString($bytes)).Replace('-', '').ToLowerInvariant()
$rng.Dispose()
```

Edit `.env`: replace `NQ_API_KEY` with the generated value. On Linux, set
`PUID` and `PGID` to the output of `id -u` and `id -g`; defaults are 1000.
Set `TZ` to your time zone, for example `Europe/Istanbul`.

Load the image and start (the same commands work in PowerShell):

```sh
docker load -i NewsQueue-1.0.9-linux-amd64.tar.gz
docker compose up -d --no-build
docker compose ps
```

Open **http://localhost:8480/** and sign in with `NQ_API_KEY`. Add your Usenet
servers under **Tools → Options → Servers**, then add NZBs. Choose
**View → Caustica / Classic** and **System / Light / Dark** independently.
The container contains the server, web UI, UnRAR, PAR2 and 7-Zip.

The image is supplied as a downloadable archive; there is no registry login
or `docker compose pull` step. `--no-build` uses the loaded image even when
only the release's Compose file is present. Do not use `--build` without the
source bundle below.

## Data and networking

Compose stores data next to `compose.yaml`:

| Host folder | Container path | Contents |
|---|---|---|
| `docker/config` | `/config` | Settings, queue, history, statistics and logs |
| `docker/downloads` | `/downloads` | Job payloads, partial files and resume sidecars |
| `docker/incomplete` | `/incomplete` | Reserved setting; currently unused |
| `docker/watch` | `/watch` | NZBs to import automatically |

Use container paths when changing folder settings in the web UI. The process
runs as `PUID:PGID`; existing nested data must also be writable by that user.
Do not point Windows and Docker instances at the same live configuration.
Keep `.env` and config backups private: they contain credentials.

Port 8480 is bound to loopback by default. For a remote server, an SSH tunnel
keeps that default usable: `ssh -L 8480:127.0.0.1:8480 user@server`, then open
localhost in your browser. For LAN access, deliberately change the mapping
to `8480:8480`; use an HTTPS reverse proxy for untrusted networks. The app
serves HTTP and authenticates API requests using the key.

## Stop, back up and update

```sh
docker compose logs --tail=100 newsqueue
docker compose stop
```

After stopping, back up `.env` and `docker/config`; preserve downloads and
incomplete files as needed. To update, load the new image archive, update the
`image:` tag in Compose if needed, then run `docker compose up -d --no-build`.
Keep your existing bind mounts. `docker compose down` removes containers and
the project network, while these host directories remain.

## Build from source instead

Download `NewsQueue-1.0.9-source.zip` from the release and extract it.
It contains the matching desktop, server and web source.

```sh
cd NewsQueue-1.0.9/app
cp env.example .env
# Edit .env as above.
docker compose build
docker compose up -d
```

Alternatively clone the repository and check out the immutable
`v1.0.9` tag. The multi-stage Dockerfile compiles Rust and egui/WASM;
Rust is not needed on the host. Other Linux architectures require a source
build and have not been validated for this release.

## Checksums and web-only bundle

The release includes SHA-256 sums for the container, matching source, web
bundle and deployment files. Compare `sha256sum <file>` on Linux or
`Get-FileHash <file> -Algorithm SHA256` in PowerShell against the sums file.
The Windows installer has a separate SHA-256 checksum.

`NewsQueue-1.0.9-web.zip` contains production static web files and the
Inter, IBM Plex and DejaVu font licenses. Serve its extracted directory with the matching
`newsqueue-server --web <directory>`; it needs the API and will not work by
opening `index.html` as a local file. The Windows downloads include this updated web UI too.

See [web QA results](web/QA_REPORT.md) for theme and interaction coverage.
