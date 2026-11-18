"""One-article NNTP mock for the Docker smoke test: python3 mocknntp.py PORT"""
import socketserver, sys

DATA = bytes(i % 256 for i in range(20000))


def yenc(data, name):
    out = bytearray(b"=ybegin line=128 size=%d name=%s\n" % (len(data), name.encode()))
    for b in data:
        e = (b + 42) % 256
        if e in (0, 10, 13, 61):
            out += bytes([61, (e + 64) % 256])
        else:
            out.append(e)
    out += b"\n=yend size=%d\n" % len(data)
    return bytes(out)


BODY = yenc(DATA, "payload.bin")


class H(socketserver.StreamRequestHandler):
    def handle(self):
        self.wfile.write(b"200 mock ready\r\n")
        for raw in self.rfile:
            cmd = raw.decode(errors="replace").strip()
            if cmd == "QUIT":
                return
            if cmd.startswith("AUTHINFO"):
                self.wfile.write(b"281 ok\r\n")
            elif cmd == "BODY <smoke1@mock>":
                self.wfile.write(b"222 0 body\r\n" + BODY + b"\r\n.\r\n")
            elif cmd == "STAT <smoke1@mock>":
                self.wfile.write(b"223 0 ok\r\n")
            elif cmd.startswith(("BODY", "STAT")):
                self.wfile.write(b"430 no such article\r\n")
            else:
                self.wfile.write(b"200 ok\r\n")


class S(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


S(("0.0.0.0", int(sys.argv[1])), H).serve_forever()
