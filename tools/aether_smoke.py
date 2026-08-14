#!/usr/bin/env python3
"""End-to-end smoke test of the Aether bus (`crates/oracle-aether`) against a real Aeon build.

    cargo build --release -p oracle-aether
    ./target/release/oracle-aether ../aeon/s4.bin --socket /tmp/aether-smoke.sock &
    python3 tools/aether_smoke.py /tmp/aether-smoke.sock

**A dev tool, not a gate artifact** — nothing in CI depends on the ROM existing, and this script is
never run by `cargo test`. Its job is the one thing the in-repo test suite structurally cannot do:
drive the transport from a *different language and a different process*, against a real 696 KB game
with a real 2,129-symbol listing, so the protocol is validated as a wire rather than as an API.

It deliberately shares no code with the server and no library with anything — it builds NDJSON
JSON-RPC 2.0 by hand, which also makes it the shortest readable description of the protocol we have.
See `empyrean/contract/protocol.md` for the normative version and
`docs/2026-08-14-aether-change-requests.md` for where we could not follow it.

Some expected values are pinned to the `s4.bin` built on 2026-08-14 (symbol count, ROM size, the
`Player_1` address); a rebuild that moves them is the D7 staleness scenario, not a bug in this script.
"""
import json, os, socket, stat, sys, time

SOCK = sys.argv[1] if len(sys.argv) > 1 else "/tmp/oracle.sock"


class C:
    def __init__(self, path):
        for _ in range(300):
            try:
                self.s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                self.s.connect(path)
                break
            except OSError:
                time.sleep(0.05)
        else:
            raise SystemExit("could not connect")
        self.f = self.s.makefile("rwb")
        self.id = 0

    def send(self, method, params=None, notify=False):
        msg = {"jsonrpc": "2.0", "method": method}
        if not notify:
            self.id += 1
            msg["id"] = self.id
        if params is not None:
            msg["params"] = params
        self.f.write((json.dumps(msg) + "\n").encode())
        self.f.flush()

    def call(self, method, params=None):
        self.send(method, params)
        while True:
            line = self.f.readline()
            if not line:
                raise SystemExit("connection closed")
            v = json.loads(line)
            if v.get("id") == self.id:
                return v


fails = []


def check(name, cond, detail=""):
    print(("  OK   " if cond else "  FAIL ") + name + (("  " + str(detail)) if detail else ""))
    if not cond:
        fails.append(name)


st = os.stat(SOCK)
check("socket mode is 0600", stat.S_IMODE(st.st_mode) == 0o600, oct(stat.S_IMODE(st.st_mode)))
check("socket is a socket", stat.S_ISSOCK(st.st_mode))

c = C(SOCK)
r = c.call("initialize", {"clientId": "smoke", "protocolVersion": 1,
                          "clientCapabilities": {"events": True}})["result"]
c.send("initialized", notify=True)
check("protocolVersion 1", r["protocolVersion"] == 1)
check("initialize is stamped", all(k in r for k in ("frame", "mclk", "running")), r.get("frame"))
check("symbolsLoaded true (s4.lst bound)", r["capabilities"]["symbolsLoaded"] is True)
print(f"       {len(r['methods'])} methods advertised")

st_ = c.call("emulator/status")["result"]
check("symbolCount == 2129 (real s4.lst)", st_["symbolCount"] == 2129, st_["symbolCount"])
check("romBytes == 696836", st_["romBytes"] == 696836, st_["romBytes"])

# D7: resolve, never hardcode. Player_1 is spelled FFFF8CFA and lives at bus $FF8CFA.
sym = c.call("emulator/lookup_symbol", {"name": "Player_1"})["result"]
check("Player_1 -> 0x00FF8CFA", sym["addr"] == "0x00FF8CFA", sym["addr"])
check("Player_1 raw spelling kept", sym["rawAddr"] == "0xFFFF8CFA", sym["rawAddr"])

# Run the real game for 300 frames and watch the event stream.
c.send("emulator/run_frames", {"frames": 300})
events = []
while True:
    v = json.loads(c.f.readline())
    if v.get("id") == c.id:
        run = v["result"]
        break
    events.append(v["method"])
check("run_frames 300 advanced the machine", run["frame"] == 300, run["frame"])
check("one resumed + one stopped pushed", events == ["emulator/resumed", "emulator/stopped"], events)

# A PC deep in the boot path must now resolve to a named routine.
res = c.call("emulator/lookup_symbol", {"addr": run["pc"]})["result"]
check("PC resolves to a symbol", isinstance(res.get("name"), str), f"{run['pc']} -> {res.get('name')}")

# The VDP has real content after 300 frames of a real game.
h = c.call("emulator/state_hash", {"includeFramebuffer": True})["result"]
check("vram hash is non-trivial", h["vram"] != "0x0000000000000000", h["vram"])
check("framebuffer hash present", h["framebuffer"].startswith("0x"), h["framebuffer"])

shot = "/tmp/aether-smoke.ppm"
sh = c.call("emulator/screenshot", {"path": shot})["result"]
size = os.path.getsize(shot)
check("screenshot is a full frame", size == len(f"P6\n{sh['width']} {sh['height']}\n255\n")
      + sh["width"] * sh["height"] * 3, f"{size} bytes, {sh['width']}x{sh['height']}")
with open(shot, "rb") as fh:
    px = fh.read()
check("frame is not blank", len(set(px[20:])) > 4, f"{len(set(px[20:]))} distinct byte values")

# Input reaches the game.
c.call("emulator/hold", {"buttons": ["start"]})
c.call("emulator/run_frames", {"frames": 60})
c.call("emulator/release_all")
after = c.call("emulator/state_hash")["result"]
check("state advanced past the title", after["combined"] != h["combined"])

# Free-running mode really runs.
before = c.call("emulator/status")["result"]["frame"]
c.call("emulator/resume")
time.sleep(1.0)
mid = c.call("emulator/status")["result"]
c.call("emulator/pause")
check("free-run advanced frames", mid["frame"] > before + 5, f"{before} -> {mid['frame']}")
check("free-run reports running", mid["running"] is True)
check("free-run is paced near 60Hz", 55 <= (mid["frame"] - before) <= 65, mid["frame"] - before)

# Refusing a wrong-shape listing on a real ROM (s4.debug.lst against s4.bin).
dbg = "/home/volence/sonic_hacks/aeon/s4.debug.lst"
if os.path.exists(dbg):
    e = c.call("emulator/load_symbols", {"path": dbg})
    check("s4.debug.lst REFUSED against s4.bin",
          e.get("error", {}).get("data", {}).get("binding") == "mismatch", e.get("error", {}).get("message"))
else:
    print("  SKIP  s4.debug.lst not present")

print()
print("SMOKE FAILURES:", fails if fails else "none")
sys.exit(1 if fails else 0)
