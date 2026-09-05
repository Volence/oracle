# CR-N — `emulator/status`'s `caveat` should say it also carries a ROM-freshness verdict

**Raised by:** oracle lane, 2026-09-05.
**Target:** `contract/protocol.md` §11.37 (next free) — a **description-only** amendment to
`emulator/status.result.caveat`, plus one §8 clause.
**Reviewer:** to be named by the adjudicator in the ruling itself, per the 2026-08-27 substituted-reviewer rule.

## The ask, in one line

`caveat` is already a declared §2.4 string on `emulator/status`. This asks only that its **description**
stop saying it carries the symbol verdict *specifically*, so a server may also report that **the ROM image
it holds is not the file at `romPath`** — the same shape, for the other file.

## Why this is being raised rather than just served

Nothing in the schema stops a server putting a second sentence in `caveat` today. §11.34 added the key as a
string and no validator constrains its content. **So this CR buys no new capability and is not needed to
ship the behaviour.**

It is raised because the description currently reads *"the standing symbol-freshness verdict"* and names
only the listing's failure modes. A server that also reports ROM staleness there is doing something its
fragment does not describe — **conformant by omission**, which is precisely the state that let
`emulator/step`'s `count` bounds go unenforced for ten days with every artifact green and only a vendored
JSON file disagreeing. This lane has now been bitten by that class twice and would rather spend a
description edit than carry it a third time.

## Why the capability is worth having at all — a measured save, tonight

**The stale-image warning that exists today is not ours and dies at the cutover.** It lives in
`oracle-old/linux-port/mcp/oracle_mcp.py` — the MCP shim, in the repo the cutover exists to delete.
`romFreshness`, `differsBy` and `bytesCompared` appear **nowhere** in this workspace's Rust; grep `crates/`
and the only hits are comments.

Its value is not theoretical. On 2026-09-05, answering a cross-lane diagnostic for aeon, this lane's own
emulator was holding a **742,018-byte** image while the path held **844,730** — a build from before their
landing. The shim's banner caught it on the **first call**. Without it this lane would have run a
per-scanline measurement against the wrong binary and published a confident finding about another lane's
work, to that lane. The failure mode is the expensive one: nothing errors, no method 404s, and the numbers
look fine.

At cutover that warning disappears **silently**. Nothing goes red; the banner simply stops being printed.

## The shape proposed

Exactly the §11.34 shape, applied to the ROM instead of the listing — deliberately, so there is one idea
and not two:

* the verdict answers **"is the image I hold still the file at `romPath`?"** and nothing else. It says
  nothing about whether that file is the build you meant;
* **quiet only when the bytes match.** Every other state, *including "could not check"*, produces a
  sentence — loud on unmeasurable, because "I could not look" must never render as "I looked and it is fine";
* **size is definitive only when it DIFFERS.** Matching sizes escalate to a real byte comparison. This is
  not fastidiousness: aeon reported two different builds sharing a byte count on the same evening;
* carried in the **existing `caveat`**, composed with the symbol verdict when both fire — one string, two
  sentences, ROM first, because a stale image makes the listing question moot.

## What is NOT asked for

* **No new result key.** A structured `romFreshness` object is the shim's shape and is explicitly not
  proposed: §8 item 20's closure rejects any result key a fragment does not declare, and §11.34 already
  ruled that a freshness verdict is *"an explanation, not a datum a client branches on"*. That ruling is
  followed here rather than relitigated.
* **No new method.**
* **No change to `reload_rom`**, whose own `caveat` already covers its moment.

## The §8 clause asked for alongside it

A schema cannot witness that a server *emits* a caveat when it should — the same blindness §11.27's
"required when applicable" half has. So the obligation, if adopted, rides as a §8 conformance clause: a
server advertising `emulator/status` and holding a ROM path MUST report, in `caveat`, an image that no
longer matches the file — with its own suite showing the size-differs case, the **same-size-different-bytes**
case, and the unmeasurable case.

Please rule on the description and the §8 clause **separately**, as with CR-L. Adopting the description
alone produces a documented obligation nothing checks.

## Consumer set

Enumerated across `aeon`, `aurora`, `seraph`, `sigil` and `empyrean`, excluding `node_modules`, vendored
trees and worktree copies: the only consumer of a ROM-freshness verdict is the **shim itself**, which
computes its own and is reference-only. No sibling reads `status.caveat` for content. **Adding a sentence
breaks nobody**; the risk runs the other way, since the shim's departure removes the only existing warning.

## What would have to be true for this to be wrong

* If the adjudicator holds that a `caveat` scoped by its description to one subject should get a **second
  key** rather than a widened scope, the answer is a `romFreshness` object and this CR is the wrong shape.
  I argue against it on §11.34's own reasoning, but that ruling was about symbols and could be read
  narrowly.
* If composing two verdicts into one string is judged to make `caveat` a datum clients will parse, then the
  string is doing structured work and should be structured. I think prose composed of complete sentences
  resists that, but it is a judgement.
* If the cutover is close enough that the shim's warning survives to be ported wholesale, this is
  duplicated effort — though it would still need this description to be legal to emit.
