# Banked CR text from serving `emulator/run_to_scanline` (2026-08-22)

Three things the fragments say that this server served **as written** while disagreeing with
them. None was repaired locally; each is written here in the shape a change request wants, so
the upstream conversation starts from text rather than from a transcript.

Served in `serve-run-to-scanline`: `emulator/run_to_scanline` (§6 line 855) plus the
`addr`-XOR-`symbol` alternation five other fragments declare.

---

## CR-a — `emulator/run_to_scanline`'s result cannot say where the machine stopped (audit **D-04**)

**What the contract says.** §6 line 855's result column is `line`, **`reached`**, `maxFrames`,
`caveat?`. Its sibling one line above, `emulator/run_to`, is `target`, **`reached`**, **`pc`**,
`maxFrames`, `symbol?`, `symbolDisp?`, `caveat?`. The fragment's own `$comment` already registers
the asymmetry as D-04 and says it was transcribed rather than corrected.

**What it costs, measured from the implementation.** A caller that ran to a raster coordinate in
order to inspect the machine there has to issue a second call (`status` or `registers`) to learn
the PC — and on a bus where any other client may also be driving the machine, the second call is
not guaranteed to describe the state the first one stopped at. Every other run-shaped row in §6
that stops on a condition reports where it stopped.

**Why it is not merely cosmetic.** D12's whole argument for `reached` is that *"a result that only
echoes its own input cannot distinguish 'my condition happened' from 'I gave up waiting'"*. The
same argument applies one step further: `{line: 100, reached: true}` echoes the input and the
verdict, and still says nothing about the machine. `pc` is the one field that makes the reply
self-contained.

**Mitigation this server already ships, so the CR is not urgent.** The `emulator/stopped` event
this call emits carries `pc` (§3 requires it), so the fact IS on the bus — for a **stream**
consumer. A plain request/response client that did not subscribe cannot see it.

**Proposed:** add `pc` (and `symbol?` / `symbolDisp?`) to the row and to the fragment, matching
`run_to` key for key. Additive and backward-compatible: a client that ignores them is unaffected.

---

## CR-b — the 0-511 `line` span is not video-mode-aware, and the row does not say what to do about it

**What the contract says.** `line` is `integer, minimum 0, maximum 511`, and the description is
explicit that this is deliberately wider than `emulator/scanlines`' 0-223 because *"a raster target
may legitimately sit in blanking"*.

**The gap.** Neither the row nor the fragment says what a server should do with a line its video
mode cannot produce. This core is NTSC V28 — `LINES_PER_FRAME = 262` — so lines **262-511** are
contractually legal and physically unreachable, and the two available readings (refuse with
`-32602`, or run the bound and answer `reached: false`) produce visibly different behaviour from
two servers that both conform.

**What this server does, and why.** Accepts them, runs the `maxFrames` bound, answers
`reached: false` with a `caveat` that says in words that the line cannot occur in this video mode
and names the last one that can. Refusing a value the fragment declares legal is §8's invention ban
read the other way round; `caveat` is *declared* on this row precisely because D12 gives it SHOULD
force here, so this is what it is for.

**The cost, stated honestly.** `{line: 300}` burns up to 600 frames of emulation to answer a
question that was decidable at parse time. Short-circuiting it would be cheaper but would make an
unreachable line observably *different* from a reachable line that simply never came round —
different frames advanced, a different machine at the end — for a caller who cannot tell the two
cases apart from the contract.

**Proposed, in preference order:**

1. **Say it in the row.** One normative sentence — *"a `line` the server's video mode cannot
   produce is answered `reached: false` with a `caveat`, never refused"* — costs nothing and makes
   the two readings one.
2. If the contract prefers refusal, say *that*, and say which error and what `data` names the
   reachable range, so the refusal is portable.

Silence is the only option that leaves two conformant servers behaving differently.

---

## CR-c — `emulator/read` is the only `addr`-or-`symbol` row with no `oneOf`

**What the contract says.** Every other row that takes an address *or* a symbol declares
`oneOf [{required:["addr"]}, {required:["symbol"]}]`: `run_to`, `read_memory`, `write_memory`,
`memory_hash`, `watchpoint_add`, `breakpoint_add`, `breakpoint_clear`, and (in its own vocabulary)
`lookup_symbol`. `emulator/read` declares neither the alternation nor any `required`.

**Consequence.** `{addr, symbol}` is a request `emulator/read`'s fragment **permits** and every
sibling refuses. A server must then either resolve one and silently drop the other — the exact
"told OK while acting on something else" defect §11.17 closed for unknown keys — or refuse a
request the schema accepts, which is this server narrowing a shape another conformant server takes.
There is no third option, and the row does not say which.

**What this server does.** Serves it as written: `emulator/read` keeps the permissive resolver and
resolves the symbol, while the five rows that declare the alternation now refuse both-together with
`-32602` and `data.conflictingParams`. The exemption is pinned by a test that also asserts the
fragment still lacks the keyword, so it cannot outlive its reason.

**Proposed:** add the same `oneOf` to `emulator/read`'s fragment. It reads like a transcription gap
rather than a decision — the row's `symbol` description already says only "Resolved to an address
(D7)", with no hint that pairing it with `addr` means anything. If it *was* a decision, the row
should say what `{addr, symbol}` means there.

---

## Not a CR: two corrections to the acceptance-21 survey

* **`run_to_scanline`'s consumer count.** The survey records "manual (1 aeon probe)". A sweep of
  the sibling trees finds **three** live call sites, and the two the survey missed are the
  load-bearing ones: `sigil/crates/sigil-harness/golden/ab/wavec/ab_wavec_vshot.py:26` and
  `ab_wavec_vcheck.py:29` both call `run_to_scanline {"line": 240}` — a **vblank** target, which no
  implementation built on the rendered-row hook could ever have served. (They use the bare,
  unprefixed method name, which is why a prefixed grep does not see them.) The third,
  `aeon/tools/engine_baseline_probe.py:586`, targets line 220. All three send `line` only, so the
  D-33 `maxFrames`/`max_frames` conflict is not exercised by any live consumer.
* **"a new sink plus a `pub` raw-line accessor is the clean shape".** A `BusEventSink` cannot call
  an accessor on the `System` that is driving it — the run loop holds `&mut self` — so the accessor
  half of that shape is not implementable. The gap was a missing *hook*, and it is now
  `BusEventSink::on_line_start(line, frame)`, delivered for every line of the frame including
  blanking. The survey's diagnosis of *why* the predicate path cannot serve this
  (`frame = mclk / MCLK_PER_FRAME` discards the remainder) was exactly right.
