#!/usr/bin/env python3
"""End-to-end smoke test of the Aether bus (`crates/oracle-aether`) against a real Aeon build.

    cargo build --release -p oracle-aether
    ./target/release/oracle-aether fixtures/aeon/s4.bin --socket /tmp/aether-smoke.sock &
    python3 tools/aether_smoke.py /tmp/aether-smoke.sock

**A dev tool, not a gate artifact** — nothing in CI depends on the ROM existing, and this script is
never run by `cargo test`. Its job is the one thing the in-repo test suite structurally cannot do:
drive the transport from a *different language and a different process*, against a real game with a
real listing, so the protocol is validated as a wire rather than as an API.

The launch line above names `fixtures/aeon/`, **this repo's own frozen copy** of Aeon's build
artifacts, rather than a sibling `../aeon` working tree. That tree belongs to another lane and is
rebuilt without warning, so a read from it is a read of whatever happened to be on disk at that
moment. See `fixtures/aeon/PROVENANCE.md` for which build is pinned and how the pin moves.

It deliberately shares no code with the server and no library with anything — it builds NDJSON
JSON-RPC 2.0 by hand, which also makes it the shortest readable description of the protocol we have.
See `empyrean/contract/protocol.md` for the normative version and
`docs/2026-08-14-aether-change-requests.md` for where we could not follow it.

**No expected value is pinned to a build.** Symbol count, ROM size and the `Player_1` address were
literals once, taken from a 2026-08-14 build of a live Aeon tree; all three had gone stale by the
time anyone looked. They are now derived at run time from the artifacts the server itself reports it
loaded (`romPath` / `symbolsPath`), which is both stale-proof across pin moves and a stronger claim:
that the numbers the server reports agree with the bytes it actually read. It also makes the script
correct against any ROM, not just this one.
"""
import json, os, re, socket, stat, struct, sys, time, zlib

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

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
rom_path = st_.get("romPath")
lst_path = st_.get("symbolsPath")
print(f"       romPath    {rom_path}")
print(f"       symbolsPath {lst_path}")

# `romBytes` is the length of the image the server loaded, so the image on disk is the authority.
# Derived, not pinned: the old literal 696836 was a 2026-08-14 build and the frozen copy is 719315.
if not rom_path or not os.path.isfile(rom_path):
    check("romPath names a readable image", False, rom_path)
else:
    want_bytes = os.path.getsize(rom_path)
    check(f"romBytes == {want_bytes} (size of {rom_path})",
          st_["romBytes"] == want_bytes, st_["romBytes"])

# The listing is the authority for the symbol figures. Read it here rather than pinning a number: a
# literal has to be hand-updated on every pin move, and the only way anyone ever updates one is by
# copying whatever the run just printed — which is a check that cannot fail.
#
# **Both paths are absolute as of §11.30 (CR-I), and this comment used to say otherwise.** It read:
# "the server reports `romPath` absolute but `symbolsPath` as it resolved it, which for the auto-bound
# sibling listing is RELATIVE to the server's cwd … so run the script from where the server was
# launched." That asymmetry — visible in this script's own output, which is where CR-I was found — is
# now fixed at the server's load boundary, so a consumer no longer has to share the server's working
# directory to open the listing it names. The launch-directory caveat is retired with it.
#
# A relative string would still be *opened* here (this process's cwd would resolve it), so this is not
# the gate for the rule — `crates/oracle-aether/tests/symbols_path.rs` is. What this script proves is
# the human-legible half: a consumer in another language and another process reads both paths and both
# are absolute. If either cannot be opened, the check below goes red with the path — loudly, rather
# than skipping.
lst_text = None
if not lst_path or not os.path.isfile(lst_path):
    check("symbolsPath names a readable listing", False, lst_path)
else:
    try:
        with open(lst_path, "r", errors="replace") as fh:
            lst_text = fh.read()
    except OSError as e:
        check(f"listing readable at {lst_path}", False, e)

if lst_text is not None:
    # `   2310 symbols` — the listing's own footer. `    0 unused symbols` does not match: the count
    # and the word `symbols` are not adjacent there.
    feet = re.findall(r"(?m)^\s*(\d+)\s+symbols\s*$", lst_text)
    if len(feet) != 1:
        check("listing declares exactly one `N symbols` footer", False, feet)
    else:
        want_syms = int(feet[0])
        # Equality, not `<=`. `symbolCount` counts the rows that carry an address, and a stock AS
        # listing emits addressless build metadata (ARCHITECTURE, DATE, TIME) that the footer counts
        # and `symbolCount` does not — so the two *can* legitimately differ by that much. sigil's
        # listings emit none, and `oracle-core`'s `real_s4_lst_parses_completely` asserts
        # `matches_declared_count()` for exactly these bytes. A shortfall here is therefore a real
        # change in the emitter, which is worth reddening on rather than absorbing.
        check(f"symbolCount == {want_syms} (the listing's own footer)",
              st_["symbolCount"] == want_syms, st_["symbolCount"])

    # D7: resolve, never hardcode. The listing supplies the raw spelling; the 24-bit bus mask is
    # applied here independently, because that mapping is the property under test.
    hits = re.findall(r"(?m)^\s*Player_1\s*:\s*([0-9A-Fa-f]{8})\b", lst_text)
    if len(hits) != 1:
        check("listing declares exactly one Player_1 row", False, hits)
    else:
        raw = hits[0].upper()
        bus = int(raw, 16) & 0xFFFFFF
        sym = c.call("emulator/lookup_symbol", {"name": "Player_1"})["result"]
        check(f"Player_1 -> 0x{bus:08X} (0x{raw} masked to 24 bits)",
              sym["addr"] == f"0x{bus:08X}", sym["addr"])
        check(f"Player_1 raw spelling kept (0x{raw})",
              sym["rawAddr"] == f"0x{raw}", sym["rawAddr"])

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

# `emulator/screenshot` writes a PNG. It wrote a PPM once, and these two checks were never moved
# across: the size check has been FAILING against every PNG since, and the blankness check went
# VACUOUS — it counted distinct bytes in a *compressed* stream, which is ~256 for any picture at all,
# blank included. Both are now derived from the format the server says it wrote.
shot = "/tmp/aether-smoke.png"
sh = c.call("emulator/screenshot", {"path": shot})["result"]
with open(shot, "rb") as fh:
    png_bytes = fh.read()
check("screenshot is a PNG", png_bytes[:8] == b"\x89PNG\r\n\x1a\n" and sh.get("format") == "png",
      f"format={sh.get('format')} magic={png_bytes[:8]!r}")
check("reported byte count matches the file", len(png_bytes) == sh.get("bytes"),
      f"{len(png_bytes)} on disk, {sh.get('bytes')} reported")

# Walk the chunks rather than assuming offsets: IHDR carries the true raster size, and the IDAT run
# carries the pixels. (`zlib` and `struct` are stdlib — the "no library with anything" rule is about
# not sharing code with the server, and this shares none.)
ihdr, idat, off = None, b"", 8
while off + 8 <= len(png_bytes):
    (clen,) = struct.unpack(">I", png_bytes[off:off + 4])
    ctype = png_bytes[off + 4:off + 8]
    body = png_bytes[off + 8:off + 8 + clen]
    if ctype == b"IHDR":
        ihdr = struct.unpack(">IIBB", body[:10])
    elif ctype == b"IDAT":
        idat += body
    off += 12 + clen

if ihdr is None:
    check("PNG declares an IHDR", False, "no IHDR chunk")
else:
    # `iw`/`ih`, not `w`/`h`: `h` upstream is the state_hash reply and shadowing it silently broke the
    # comparison two checks later.
    iw, ih, depth, colour = ihdr
    check(f"IHDR is {sh['width']}x{sh['height']}, 8-bit RGB",
          (iw, ih, depth, colour) == (sh["width"], sh["height"], 8, 2), ihdr)
    try:
        px = zlib.decompress(idat)
    except zlib.error as e:
        px = b""
        check("IDAT stream decompresses", False, e)
    if px:
        # 8-bit RGB: one filter byte plus three bytes per pixel, per row. A truncated or short capture
        # cannot satisfy the frame's own arithmetic, which is what "full frame" actually means.
        want = ih * (1 + iw * 3)
        check(f"decoded raster is a full frame ({want} bytes)", len(px) == want,
              f"{len(px)} bytes, {iw}x{ih}")
        # Now a real test: these are pixel bytes, so a blank frame genuinely collapses to a handful of
        # distinct values. The old form ran on compressed bytes and could not fail.
        check("frame is not blank", len(set(px)) > 4, f"{len(set(px))} distinct pixel byte values")

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

# Refusing a wrong-shape listing on a real ROM (s4.debug.lst against a release image). The frozen
# copy, not aeon's live tree: this used to be an absolute path into another lane's working directory,
# where the file could be any build or absent, and absent read as a printed SKIP.
#
# It is only a *cross* for a release image. On a debug image it is the correct listing and would be
# accepted, so name that case and skip it rather than let a meaningless FAIL stand.
dbg = os.path.join(REPO, "fixtures", "aeon", "s4.debug.lst")
if os.path.basename(rom_path or "").endswith(".debug.bin"):
    print("  SKIP  server is on a debug image — s4.debug.lst is its matching listing, not a cross")
elif not os.path.isfile(dbg):
    check("frozen fixtures/aeon/s4.debug.lst is present", False, dbg)
else:
    e = c.call("emulator/load_symbols", {"path": dbg})
    check("s4.debug.lst REFUSED against a release image",
          e.get("error", {}).get("data", {}).get("binding") == "mismatch", e.get("error", {}).get("message"))

print()
print("SMOKE FAILURES:", fails if fails else "none")
sys.exit(1 if fails else 0)
