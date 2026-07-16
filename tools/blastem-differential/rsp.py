#!/usr/bin/env python3
"""Clean-room GDB Remote Serial Protocol client for BlastEm's `-D` stub.

This is the transport for the "BlastEm-over-the-bus" behavioral differential
(docs/plans/2026-07-16-m68000-blastem-differential.md). It speaks the standard
GDB Remote Serial Protocol over BlastEm's stdio (the `target remote | blastem
ROM -D` form) and NEVER touches BlastEm source — behavior enters only through the
protocol and the observable side effects of a harness ROM.

BlastEm is launched under `xvfb-run` so each run gets an isolated, disposable
headless X display (repeated windowed sessions on a shared :0 are unreliable).

Environment:
  BLASTEM   path to the blastem binary   (default: ../../../emulators/blastem64-0.6.2/blastem)
Notes on this stub (BlastEm 0.6.2, discovered empirically, black-box):
  * register block `g` = 18 x 32-bit words: d0-d7, a0-a7, sr, pc.
  * `G` (write-all-registers) CRASHES the stub; use `P` (write one register).
  * `P` for pc (reg 17) returns E01 (unsupported); drive PC via a RAM-dispatch ROM.
  * a command that "times out" for us may still be answered late by the stub, so
    every command drains stale input first to keep request/reply in lockstep.
"""
import os
import select
import signal
import subprocess

_HERE = os.path.dirname(os.path.abspath(__file__))
BLASTEM = os.environ.get(
    "BLASTEM",
    os.path.normpath(os.path.join(_HERE, "..", "..", "..", "emulators", "blastem64-0.6.2", "blastem")),
)


def cksum(b):
    return sum(b) & 0xFF


class RSP:
    def __init__(self, rom, extra=()):
        env = {**os.environ, "SDL_AUDIODRIVER": "dummy"}
        cmd = ["xvfb-run", "-a", "-s", "-screen 0 640x480x24",
               BLASTEM, rom, "-D", "-g", *extra]
        self.p = subprocess.Popen(
            cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, bufsize=0, env=env)

    def _rb(self, t):
        r, _, _ = select.select([self.p.stdout], [], [], max(t, 0.0))
        if not r:
            return None
        return self.p.stdout.read(1)

    def _drain(self):
        """Discard pending input (late replies from timed-out commands)."""
        while True:
            r, _, _ = select.select([self.p.stdout], [], [], 0.02)
            if not r or not self.p.stdout.read(1):
                return

    def recv(self, t=6.0):
        import time
        dl = time.time() + t
        while time.time() < dl:
            b = self._rb(dl - time.time())
            if b is None:
                return None
            if b == b'$':
                break
        else:
            return None
        body = b''
        while True:
            b = self._rb(dl - time.time())
            if b is None:
                return None
            if b == b'#':
                break
            body += b
        self._rb(0.5)
        self._rb(0.5)                       # 2 checksum bytes
        self.p.stdin.write(b'+')
        self.p.stdin.flush()
        return body

    def cmd(self, c, t=6.0):
        if isinstance(c, str):
            c = c.encode()
        self._drain()
        self.p.stdin.write(b'$' + c + b'#' + f"{cksum(c):02x}".encode())
        self.p.stdin.flush()
        return self.recv(t)

    # --- readiness -----------------------------------------------------------
    def wait_ready(self):
        """Wait for the stub to halt at entry (covers BlastEm/xvfb startup)."""
        rep = self.cmd('?', 12.0) or self.cmd('?', 6.0)
        if not rep:
            raise RuntimeError("BlastEm RSP stub never became ready")
        self._drain()
        return rep

    # --- registers (g-packet order: d0-7, a0-7, sr, pc) ----------------------
    def read_regs(self):
        g = self.cmd('g')
        assert g and len(g) >= 18 * 8, "no/short reg reply"
        w = [int(g[i * 8:i * 8 + 8], 16) for i in range(18)]
        return {'d': w[0:8], 'a': w[8:16], 'sr': w[16] & 0xFFFF, 'pc': w[17]}

    def write_reg(self, n, value):
        return self.cmd(f'P{n:x}={value:08x}')

    def set_sr(self, sr):
        return self.write_reg(16, sr)

    def set_d(self, i, v):
        return self.write_reg(i, v)

    def set_a(self, i, v):
        return self.write_reg(8 + i, v)

    # --- memory --------------------------------------------------------------
    def read_mem(self, addr, ln):
        d = self.cmd(f'm{addr:x},{ln:x}')
        return bytes.fromhex(d.decode()) if d else None

    def write_mem(self, addr, data):
        return self.cmd(f'M{addr:x},{len(data):x}:' + data.hex())

    # --- execution -----------------------------------------------------------
    def step(self):
        return self.cmd('s')

    def bp(self, addr):
        return self.cmd(f'Z0,{addr:x},2')

    def cont_or_interrupt(self, t=4.0):
        """Continue; if the target does not stop within t (a STOPped CPU never
        will), send an RSP break (0x03) to regain control.
        Returns (stop_reply_or_None, timed_out)."""
        c = b'c'
        self._drain()
        self.p.stdin.write(b'$' + c + b'#' + f"{cksum(c):02x}".encode())
        self.p.stdin.flush()
        rep = self.recv(t)
        if rep is not None:
            return rep, False
        self.p.stdin.write(b'\x03')          # RSP interrupt
        self.p.stdin.flush()
        return self.recv(4.0), True

    def close(self):
        try:
            self.cmd('k', 1.0)
        except Exception:
            pass
        try:
            self.p.kill()
        except Exception:
            pass


def watchdog(seconds):
    """Guarantee the process exits even if the stub wedges."""
    signal.signal(signal.SIGALRM, lambda *_: (_ for _ in ()).throw(SystemExit("watchdog")))
    signal.alarm(seconds)


if __name__ == '__main__':
    # Smoke test: boot the shipped menu ROM and read architectural state.
    watchdog(45)
    rom = os.path.join(os.path.dirname(BLASTEM), "menu.bin")
    r = RSP(rom)
    try:
        r.wait_ready()
        reg = r.read_regs()
        print("regs=%d sr=%04x pc=%08x" % (18, reg['sr'], reg['pc']))
        r.write_mem(0xFF0000, bytes.fromhex('DEADBEEF'))
        print("ram roundtrip:", r.read_mem(0xFF0000, 4).hex())
        print("OK")
    finally:
        r.close()
