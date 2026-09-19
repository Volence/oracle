# F-FIRST-PRESENT-REFUSAL — the publish-before-pump obligation, proposed as contract text

**Status: A PROPOSAL. Not landed text.** Nothing here has been applied to `contract/`, which is the hub's
tree; this lane does not touch it. §3 below is the sentence set being handed over for the hub to rule on.
Everything else is the grounds.

**Filed by:** oracle lane, 2026-09-19. **Server half landed at** `parcel/first-present-order`
(`oracle-firstpresent`). **Anchors re-verified firsthand at that branch**, not transcribed.

---

## 1. The defect, stated once

**A window that exists answered "there is no window", once, at the start of every session.**

Both embedders drained the hosted bus once per iteration, **after** that iteration's frame and **before**
its publishes. One `Host::pump` answers every request it finds queued. So on iteration 1 — and only
iteration 1 — a request the drain answered was served:

* `emulator/screen_text` → `-32005`, `error.data.reason = "noDisplay"`, whose message is *"this server has
  no window; screen text exists only in a hosted player"*;
* `emulator/status` → `"display": false`;
* `emulator/pacing` → `-32005`, `reason = "noPacing"` (`oracle-player` only; `oracle-frontend` publishes no
  pacing at all);

from a live window that had **already run frame 1** and was composing it for the glass. The refusal
reproduced in this parcel's gate is byte-identical to the one CI run 34752339602 caught as a flake:
`{"code":-32005,"data":{"frame":1,"mclk":896042,"reason":"noDisplay","running":true}}`.

### Why the obvious fix is wrong

"Publish before the first drain" cannot work. In iteration 1 **there is nothing on the glass yet**, and both
publishes exist precisely to answer *the frame that is on the glass* (`Host::set_screen_text`'s own doc).
Publishing early would trade a false *"no window"* for a false *picture*, which is worse: a caller can
recover from a refusal and cannot detect a plausible wrong answer.

So the repair is the **order**, and only for iteration 1: that one drain moves to behind the iteration's
publishes. Every later iteration keeps today's position.

### Why a contract sentence is owed

Nothing in the protocol or in `Host` can detect the mistake. `Host::pump` has no way to know whether a
window exists, let alone whether it has composed a frame; `set_screen_text` is an optional push. So the
honesty of `noDisplay`, `noPacing` and `status.display` rests entirely on a behaviour **the contract does
not record** — and a third embedder would get it wrong in exactly the same silent way, with nothing on
either side able to notice. This is the class the oracle suite measured five separate times in one night:
a signal whose truth is a convention.

---

## 2. What this proposal does NOT ask for

* **No change to what `display` means.** The hub has adopted a redefinition of its description; that edit is
  the hub's and lands separately. The obligation below makes the field's *existing* behaviour stop
  misreporting; it does not touch the field, its type, or its optionality.
* **No new method, no new field, no new refusal code.** `noDisplay` and `noPacing` keep their codes,
  discriminants and messages.
* **No obligation on a headless server.** A server with no window publishes nothing and must keep answering
  `display: false` / `noDisplay` immediately. The obligation below is conditional on *being* a windowed
  embedder, which is the only way to state it without breaking `oracle-aether` standalone.

---

## 3. The proposed text

Normative register, testable, naming what an embedder must guarantee rather than how. Suggested home: the
hosted-embedder obligations (the `D`-numbered list), cross-referenced from §11.29 and §11.42. Numbering is
the hub's to assign; `Dn` below is a placeholder.

> **Dn — A hosted window must not answer before it has published what it shows.**
>
> A host that serves any of the window-state readbacks — `emulator/screen_text` (§11.29), the
> `emulator/status` rider `display` (§11.29), `emulator/pacing` (§11.42) — is a **windowed embedder**. A
> windowed embedder MUST NOT pump the bus before it has published, at least once, the state those readbacks
> report. Concretely, for the whole life of the process:
>
> 1. **No pump before the first publish.** The first `pump` that answers any request MUST be preceded by the
>    embedder's first push of the window state it serves (for `emulator/screen_text`, the snapshot; for
>    `emulator/pacing`, the pacing figures). An embedder whose loop reaches its drain before its first
>    publish MUST defer that drain until after the publish, and MUST NOT satisfy this rule by publishing
>    earlier.
> 2. **No publish of an uncomposed frame.** A published snapshot MUST describe a frame the embedder has
>    finished composing. It MUST NOT be primed, placeheld, or synthesised to satisfy (1): a readback that
>    describes a frame that was never composed is a worse answer than a refusal, because a caller can detect
>    a refusal and cannot detect a plausible wrong one.
> 3. **The deferral is bounded.** An embedder MUST NOT withhold pumps indefinitely waiting to publish. At
>    most one drain may be deferred, and the deferred drain MUST run in the same iteration or the next one.
>    (A rule keyed on *"has this window published yet"* would otherwise starve the bus for ever in a
>    windowed embedder that never publishes — for instance one whose publish is gated on serving a socket.)
>
> **Therefore:** `reason: "noDisplay"`, `reason: "noPacing"` and `display: false` from a windowed embedder
> mean what they say — *there is no window / no pacing to read* — and never *"you asked before the first
> frame"*. A client MUST NOT have to poll `display` to work around a host's iteration order. A headless
> server (no window) is not a windowed embedder and continues to answer `display: false` and `noDisplay`
> immediately and permanently.
>
> **Conformance.** The obligation is not checkable from the wire alone in one exchange — a single
> `noDisplay` is indistinguishable from the honest answer. It is checked by an embedder-side gate: a request
> queued **before** the embedder's first iteration must be answered by a window that exists. Each windowed
> embedder owes one such gate.

### The one sentence, if only one is taken

> A windowed embedder MUST NOT pump the bus before it has published the window state its readbacks report,
> and MUST NOT publish a snapshot of a frame it has not composed.

---

## 4. Anchors — where this is now true in the server

| What | Where |
|---|---|
| Player: the drain's normal position, and the deferral raised there | `crates/oracle-player/src/main.rs`, `Loop::iterate` |
| Player: the deferred drain, immediately behind `publish_screen_text` | same, after the `if serving { self.publish_screen_text(..) }` block |
| Player: the pump and every reaction, one method so two call positions cannot drift | `Loop::drain_and_react` |
| Player gate (measured, real socket, real `Loop::iterate`) | `a_request_queued_before_the_first_frame_is_not_told_there_is_no_window` |
| Frontend: the rule as an object | `crates/oracle-frontend/src/drain.rs`, `drain::Order` |
| Frontend: the two call positions | `crates/oracle-frontend/src/main.rs`, the run loop's drain site and the bottom of the loop body |
| Frontend gate (differential over a real socket) | `a_request_that_beats_the_first_present_is_not_told_there_is_no_window` |
| Frontend gate (the bound in Dn.3) | `the_loop_defers_its_first_drain_and_only_its_first` |
| The obligation restated at the host, where an embedder will read it | `crates/oracle-aether/src/host.rs`, `Host::set_screen_text` |

## 5. What the server half is blind to, stated for the hub

* **Neither embedder's `fn main`/`update` call sites are fully observed.** `oracle-frontend`'s loop needs a
  `minifb::Window`, so its gate drives `drain::Order` in the order the loop is written to use, not the loop
  itself — the residue `FRONTEND-LOOP-UNTESTABLE` already recorded. `oracle-player`'s gate *does* drive the
  real `Loop::iterate`.
* **Multi-pass composition is not narrowed by this parcel.** `egui::Context::run` re-runs the player's whole
  closure when a pass requests a discard (measured: one `turn` in the player's own fixture calls `iterate`
  twice). So a client can be answered from a composition egui subsequently discarded. That is a standing
  property of *every* publish in that loop, older than this defect, and Dn.2 above is deliberately written
  as "a frame the embedder has finished composing" rather than "a frame that was presented" because the
  stronger wording is not something either embedder can currently promise.
