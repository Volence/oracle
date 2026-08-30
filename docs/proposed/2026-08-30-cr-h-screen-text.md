# CR-H — `emulator/screen_text`: read what the player window actually says

**Filed by:** oracle lane, 2026-08-30. **Grounds:** `docs/2026-08-30-overlay-state-recon.md`
(oracle `5490022`, merged `02b9388`). Every code anchor below was re-verified firsthand at that
revision by the filing seat, not transcribed from the recon.

## The ask

A tool cannot read the player's on-screen text. Today the only instrument is a human looking at the
window, which is why a sibling lane keeps asking the owner to eyeball things and why our own checks
read screenshots. **One new method.** No change to any existing fragment except one optional rider
(§6).

## 1. Why it cannot ride an existing method

* **`emulator/player_state` is a false friend** (`engine.rs:564`): it is the *game's* player pool —
  Sonic's slots — not the media player. It is the obvious-looking home and the wrong subject. Named
  here because the filing seat's own first guess was wrong, and the next reader's will be too.
* **`emulator/screenshot` returns pixels**, which is precisely the eyeballing this removes.
* No `$defs` in the vendored contract covers a text-line list. `boundedList` is the right envelope;
  **the item shape is genuinely new**, which is what makes this a CR rather than a build.

## 2. The name is `screen_text`, not `overlay_state`

The queue item is called OVERLAY-STATE and **the method should not be.** The recon found that one of
the text surfaces is the **window title bar** (`main.rs:2105-2110`, `window.set_title`), which is not
part of the overlay at all — it is drawn by the window manager, and it is invisible both to a source
enumeration of the overlay and to any OCR of the presented framebuffer. **A method named `overlay_*`
invites exactly the omission that was nearly made here.** The subject is *text a human can read on
the player*, so the name says that.

## 3. Shape

Derived from what a caller needs, not from what is easy to serve.

```jsonc
// emulator/screen_text  — params: {}          (no params; the screen has one state)
{
  "surfaces": [                                 // boundedList; stable order, back to front
    {
      "kind": "statusLine",                     // statusLine | toast | palette | lens | titleBar
      "text": "AETHER ON  320x224  F 12720",    // the SOURCE string the player composed
      "rendered": "AETHER ON  320x224  F 1",    // what is ACTUALLY on the glass, after truncation
      "truncated": true,                        // rendered != text
      "unrenderable": ["`", "—"]           // chars in `text` with no glyph (see §5)
    }
  ]
}
```

### Why BOTH `text` and `rendered`, rather than a choice

The recon was asked to pick one and correctly refused, and the reasoning is the load-bearing part:

* **Rendered-only** reports the message's shadow. A caller checking *"did the player tell the user why
  the ROM failed to open"* sees `…/LOCKED (PE` and cannot tell that `Permission denied` was lost.
* **Source-only is structurally blind to the entire defect class.** It would report text that is *not
  on screen* as though it were, which makes the tool useless for the one question it exists to answer
  — *is this window lying to me?*

Serving both costs near nothing: `fit` already returns a **borrowed prefix**, so `rendered` is a slice
of a string that was composed anyway. **`truncated` is a derived convenience and is NOT the guard** —
a caller that wants the honest reading compares the two fields.

## 4. ⚠ The refusal, and why an empty list is forbidden

**When no window exists, this method REFUSES: `-32005`, `data: { "noDisplay": true }`.**

It must not return `surfaces: []`. The same `METHODS` list is served by a headless server *and* by the
player (`oracle-frontend/src/bus.rs:105-124`), and **"a window showing no text" is the default launch
state and already means something else.** An empty list would make "there is no screen" and "the screen
is blank" the same artifact — this suite's recurring defect, and the exact shape of the bar that says a
silent skip and a pass are indistinguishable.

**Rider (§6):** `emulator/status` gains `display: boolean`, so a caller can *ask* rather than probe by
failing. This is the only change to an existing fragment and it is additive.

## 5. `unrenderable` — a field neither obvious option would have carried

Verified firsthand under a positive control (`'A'` present; backtick and em dash absent from
`font.rs`): **the player has no glyph for `` ` `` or `—`, and its own very first toast contains a
backtick** (`main.rs:1207`), so it renders a hollow box today. Six live em-dash toasts do the same.
`font.rs`'s guard test restates its own input, so it cannot catch either.

A tool reading `rendered` would report a string whose characters are not the ones on the glass.
`unrenderable` is what lets a caller say *"the window is showing a box where this character should
be"* — and it turns a class of defect that currently has no observer into one a test can assert on.
**This is the better-than-the-floor half of the CR**: neither `text` nor `rendered` alone can express it.

## 6. ⚠ What this method must NOT do: join to `frame`

**`F-WINDOW-BUS-FRAME-OFFBYONE` is now DIAGNOSED, and its registered "probably a harmless convention
difference" is DISPROVEN.** Two independent quantities:

* **bus** `frame` = `now() / MCLK_PER_FRAME` (`engine.rs:2223`) — derived from the clock.
* **window** `F` = a counter incremented after *every* run iteration whether or not a frame completed
  (`main.rs:1929`).

A breakpoint that stops mid-frame gives a **permanent +1**, and the anchor snaps back so it never
self-corrects. **A state load diverges them without bound in the other direction** — the window prints
*"frame counter continues at {frame}"* while the restored clock rewinds.

So the status line's `F 12720` is **a UI counter, not the machine's frame** — and `engine.rs:2237`
already names "a UI counter" as the thing it was refusing to serve. **This CR therefore serves the
window's text as TEXT and makes no claim that any number inside it corresponds to a bus field.** The
fragment says so in its own description, because a consumer will otherwise join them, and the join is
silently wrong rather than loudly wrong.

**Fixing the counter is deliberately NOT in this CR.** It is a separate defect with a separate owner;
bundling it would make a contract change contingent on a behaviour change.

## 7. Safety — the property that must not break

This repo's structural guarantee is that display concerns cannot perturb emulation: the render that
commits sprite-overflow/collision latches takes no display mask and has no masked twin, so the
guarantee is enforced by the type system rather than by tests.

**This design preserves it trivially, and the reason is architectural rather than careful:** the
served value is a snapshot of strings the frontend *already built for drawing*, pushed once per
present through the same seam shape as `Host::set_live_pads` (`host.rs:315`). It touches no `Vdp` and
no `System`, and it sits outside `state_hash` for the same reason `LayerMask` does.

**Compose-on-demand is refused.** Having a handler ask the frontend to *build* the text when a caller
asks would run UI composition at an arbitrary point in the frame, which is the one version of this
that could perturb anything.

## 8. What was NOT validated, stated rather than glossed

**No contract vectors accompany this CR.** The filing seat's own bar, earned on CR-F, is that an
artifact authored against a schema is run against that schema before handover — and vectors cannot be
run until the fragment's `$defs` exist. **Shipping unvalidated vectors is the failure this names**, so
they are deliberately absent rather than present-and-unchecked. Vectors will be derived from a real
reply and run against the schema before they are handed over.

**Also unverified by this seat, and flagged:** the recon's claim that hosted handlers run on the
frontend's own main thread was confirmed at the source (`Host::pump` takes `&mut System`
synchronously, `host.rs:455`), but **no runtime check was performed** — this lane does not drive the
owner's live player. If adjudication turns on it, that is a named runtime item.
