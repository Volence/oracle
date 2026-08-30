# OVERLAY-STATE — design recon: letting a tool read the player's on-screen text

**Date:** 2026-08-30 · **Branch:** `recon/overlay-state` · **Base:** `8f6fe65` · **Status:** recon only —
no bus method, no wire field, no contract edit was written, and none should be until the contract rules
(§7.5).

**Runtime used: none.** No emulator MCP call, no player launch, no `cargo` run. Every claim below is
derived from source at this revision. The two items that would benefit from runtime are tagged ⟨RUNTIME⟩ in §13 and
are foreground follow-ups, never a subagent.

---

## 0. The ask, and who is actually asking

The player window draws text. Today the only way to know what it says is for a human to look at it. A
sibling lane (aeon, on EFFECTS-W1) keeps asking the owner to eyeball things, and our own runtime checks
read screenshots — `docs/2026-08-29-window-runtime-checks.md` §1 names this row by name.

**There are two callers, and they want different things.** Naming both up front, because §8 and §11 are
decided by which one you serve:

* **Caller A — the sibling lane.** "What is the player telling me right now?" Wants *meaning*: the
  message, whole, whether or not it fit on the glass.
* **Caller B — this repo's own screen-honesty checks.** "Does the glass say what we think it says?"
  Wants *the pixels' story*: the string that survived layout, so a defect like `F-TOAST-TRUNCATES` is
  visible to a test instead of to a person.

A design that serves only A is structurally blind to the entire class of defect that motivated
`F-TOAST-TRUNCATES`. A design that serves only B reports text that is not the message. **Serve both,
side by side, and label which is which.** The decision is made explicitly in §11.

---

## 1. Corrections to the brief

The dispatch brief offered three facts as hypotheses. Two hold, one does not, and one framing needs
sharpening.

1. ✅ **`emulator/player_state` is the game's player pool, not the media player.** Confirmed —
   `crates/oracle-aether/src/engine.rs:564` registers it and the handler reads Sonic/Tails object slots.
   It is not a candidate home, and the name collision is real: anyone grepping `player` for "the media
   player's state" lands there first.
2. ⚠ **`MachineInfo` is NOT a state slot.** The brief called it "the one existing frontend→host state
   seam." It is a **transport struct that is destructured on arrival and never stored**
   (`crates/oracle-aether/src/host.rs:232-236` decomposes it into `Engine::set_rom_path` /
   `Engine::set_symbols`). Widening it is therefore not "adding a field to the shared state"; it is
   adding a field to a one-shot constructor argument that is pushed twice in the whole program's life
   (launch, `bus.rs:113`; F5 ROM reload, `main.rs:1737`). It is the wrong seam for per-frame state —
   see §7.1.
3. ❌ **It is not the *one* frontend→host seam.** There are at least four already, and three of them are
   exactly the shape this feature needs: `Host::set_paused` (`host.rs:284`), `Host::set_live_pads`
   (`host.rs:315`), `Host::set_layer`/`layers`, and `Host::publish_capture` (`host.rs:433`, which hands the bus a
   whole captured frame every iteration). **`set_live_pads` is the precedent to copy.** The brief's
   central question — "how does a bus handler reach frontend state?" — is already answered four times in
   this tree.
4. ⚠ **The `overlay.rs:275` comment is not about a global composition point.** It is scoped to one badge
   (`Overlay::layer_badge`, `overlay.rs:299`), and says that *that badge's* text and geometry are derived
   once so the reservation and the paint cannot disagree. It says nothing about the frame-wide text path.
   The real answer to "is there one composition point" is §5, and it is **no**.

---

## 2. The one primitive

Every glyph on the glass comes from **`font::Canvas::text`** —
`crates/oracle-frontend/src/font.rs:175`:

```rust
pub fn text(&mut self, x: i32, y: i32, px: usize, color: u32, text: &str) -> usize
```

5×7 bitmap, uppercase-folded, **unmapped characters draw as a hollow box (`MISSING`, `font.rs:26`)**, and
it clips at the *buffer* edge only — never at a panel rect, so every caller fits its own text.

Two truncators sit above it:

* **`overlay::fit(text, avail, px) -> &str`** (`overlay.rs:425`) — the longest **prefix** whose ink fits.
  Right-truncation, UTF-8-safe, **silent**: it returns a shorter string, never an error. Used by the
  status line, toasts, and every palette row.
* **`lens::cpu::fit_tail`** (`lens/cpu.rs:53`) — the only *front*-truncator, prefixing `"..."`. Used for
  the PC symbol line only.

Because `fit` returns a borrowed prefix of its input, **the source string and the rendered string are both
available at draw time and are related by a pure function.** That is what makes §8's answer cheap.

---

## 3. Enumeration of text surfaces

Seventeen non-test `Canvas::text` call sites, ~67 invocations on a frame with everything on. Grouped by
the surface a person would name.

### 3.1 Overlay (`crates/oracle-frontend/src/overlay.rs`), entry `Overlay::draw` @ 243

| Surface | Site | String source | Truncation | Lives past the frame? |
|---|---|---|---|---|
| **Layer badge** `HIDDEN: planeB window` | `overlay.rs:335` | `format!` in `Overlay::layer_badge` @304, from `Status.layers: LayerMask` | **Never truncated.** Font scale steps down, then the badge is dropped whole — `"HIDDEN: plan"` names a layer that does not exist | no |
| **Status line (F3)** `VOL 8/10 AETHER ON AUDIO VA2 4:3 320X224 F12720` | `overlay.rs:382` | `overlay::status_text(st)` @464 — one `format!` over `Status` | **Yes, silently**, `fit` @370. Field order *is* priority order (honesty fields first) | no |
| **Toasts** (≤5, ~2.5 s each) | `overlay.rs:410` | `Toast.text: String` in `Overlay.toasts: VecDeque<Toast>` @130 | **Yes, silently**, `fit` @397 | **YES — the only retained strings** |
| **Save-slot strip** (10 digits) | `overlay.rs:537` in `draw_slot_strip` @509 | loop index over `Status.occupied: [bool; 10]` | n/a (1 char) | no |
| **PAUSED banner** | `overlay.rs:645` in `draw_paused_banner` @640 | `const PAUSED_WORD` @544 | **Never** — `banner_layout` @550 returns `None` rather than draw `PAU` | const |

### 3.2 Command palette / ROM browser (`palette.rs`), entry `Palette::draw` @ 288

All rows pass through `overlay::fit`. Two modes.

| Surface | Site | String source |
|---|---|---|
| Picker title | `palette.rs:324` | `Picker.title` @40 — `"SELECT SLOT"` (`main.rs:1491`), `format!("OPEN ROM - {}", dir.display())` (`main.rs:514`) |
| Picker filter line `> q_` | `palette.rs:329` | `Picker.query` |
| Picker rows | `palette.rs:348` | `Picker.items: Vec<(String, Cmd)>` |
| Command query line `> q_` | `palette.rs:356` | `Palette.query` @70 |
| Group headers | `palette.rs:378` | `Group::title()` + literal `"RECENT"` @142 |
| Command titles | `palette.rs:395` | `CommandInfo.title` from `commands::registry()` |
| Hotkey column | `palette.rs:413` | `commands::key_name(k)` — **not** fitted; suppressed whole if too wide @409 |

**`rom_browser.rs` has zero `.text(` calls and is not a surface.** It is a model:
`rom_browser::picker_label` @112 builds the row string (`"{label}   [loaded]"`), `main.rs::open_rom_picker`
@486 hands it to `palette.open_picker` @514, and **`palette.rs:324`/`:348` render it.** Same for
`pick.rs` (zero text calls): its `Pick.toast` reaches the glass only through `ov.push` at `main.rs:1289`,
i.e. as an ordinary toast.

### 3.3 Lenses (`lens/`), entry `lens::draw` @ `lens/mod.rs:370`

| Lens | Site | Content | Truncation |
|---|---|---|---|
| **Watch ticker** | `lens/watch.rs:118` (header), `:126` (≤4 rows) | `WATCH armed 2 dropped 0`; `w0 vram $4A00 3F->12 @f811 Sonic_Move+$1C` from `Ticker.lines` | `fit` |
| **CPU chip** | `lens/cpu.rs:299` (3 or 11 rows) | `PC <symbol>`, `SR $2700 S7`, `F <frame>`, D/A registers | row 0 uses `fit_tail` (front-cut, `...`); the rest use `fit` but `fixed_width` @161 guarantees they never actually cut |
| **Profiler** | `lens/profile.rs:230` (≤12 rows) | `PROFILER ARMED frames 120 FLOORED`, cost rows, `VINT`/`HINT`, or the literal `"(no whole frames sampled yet)"` @116 | `fit` @237; the whole panel bails rather than clip @221 |
| **Hover callout** | `lens/video.rs:445` (1 row) | `plane A \| tile $0A3 \| pal 2 \| pri 1` from `Hover.text` @259 | `fit` @450; draws nothing if no placement clears the status band |

`video::draw_cram` @69 and `video::draw_sprites` @209 are swatches and outlines — **no text**.

### 3.4 ⚑ The surface nobody listed: the **window title bar**

`crates/oracle-frontend/src/main.rs:2105-2110`:

```rust
let title = if paused {
    format!("Oracle — frame {frame} [PAUSED]")
} else {
    format!("Oracle — frame {frame}")
};
window.set_title(&title);
```

This is on-screen text a human reads, it carries the pause state and the frame counter, and it is drawn by
the **window manager, not by `Canvas`** — so it is invisible to any design that enumerates `.text(` call
sites, invisible to a screenshot of the client area, and invisible to any pixel-level OCR of the presented
buffer. It is also the *only* text surface that is always visible regardless of F3, lenses, or toasts.

**It belongs in the readout.** Costs one more record; omitting it means a caller that asks "is it paused"
and gets an empty overlay has been told nothing, while the title bar was saying `[PAUSED]` the whole time.

### 3.5 Two live defects the enumeration turned up

Not this parcel's to fix, but they change §8's design and should be registered:

* **`F-FONT-BACKTICK`** — the **very first string the player ever shows** is
  ``ov.push("PRESS ` FOR COMMANDS", INFO)`` (`main.rs:1207`). The backtick has **no glyph**
  (`font.rs:30-99` has `'`, `"`, `^` and no `` ` ``), so it renders as a hollow box. The guard test
  `the_characters_the_frontend_prints_all_have_glyphs` (`font.rs:238`) asserts over a **hand-written
  literal** that also omits the backtick, so it is structurally incapable of catching this. *A test that
  restates the input cannot check the input.*
* **`F-FONT-EMDASH`** — U+2014 has no glyph either, and it is in live toast text:
  `main.rs:1397`, `:1445` (`"watch cleared — no longer recording writes"`, verified firsthand), `:1599`,
  `:1688`, `:1703`, `:1996`, and in `symbol_watch.rs`'s per-value-change toasts — which fire repeatedly
  during play.

**Why this matters to the design:** a readout that serves the *source* string reports
`PRESS ` FOR COMMANDS`; a readout that serves the *fitted* string reports the same thing, because `fit`
truncates and does not transliterate. **Neither field can show a missing-glyph box.** §8.3 proposes the
cheap honest fix.

---

## 4. What is NOT text, and must not be sold as text

`draw_slot_strip` draws ten digits, but the *information* is which slot is selected and which are
occupied — carried in colour and in a filled box, not in the characters. A readout that reports
`"0123456789"` has told the caller nothing. Serve it as the strip's **model** (`slot`, `occupied[]`) or
not at all; do not serve its glyphs and call it text.

Same for `video::draw_sprites` outlines and `video::draw_cram` swatches: on screen, carrying meaning, not
text. Out of scope, and say so on the wire rather than letting a caller infer the screen was blank.

---

## 5. Is there a single composition point? — **No. Three, plus the title bar.**

The present block, `crates/oracle-frontend/src/main.rs` ~2105-2231, in order:

```
2110  window.set_title(&title)                     ← surface 4 (WM-drawn)
2121  present::scale_into(...)                     ← the game picture into `screen`
2135  ov.tick(paused)                              ← ages toasts, counts the pause dwell
2192  lens::draw(&mut screen, …, &models)          ← surface 3   (gated: `if cfg.lenses.any() || paused`)
2203  palette.draw(&mut screen, …, &reg)           ← surface 2   (self-gated on `is_open`)
2204  ov.draw(&mut screen, …, &Status { … })       ← surface 1   (self-gated on `showing_status`)
2231  window.update_with_buffer(&screen, …)
```

Three independent top-level draw calls into one `screen` buffer, in Z order, each anchored to
`present_view` (the *picture* rect, not the window). They are arbitrated not by a composer but by
**exported geometry accessors**: `overlay::status_band` @629 and `overlay::paused_banner_rect` @593 are
read by `cpu::top_of` (`lens/cpu.rs:240`), `profile::floor_of` (`lens/profile.rs:180`) and
`video::draw_hover` (`lens/video.rs:407-425`) so the lenses step out of the overlay's way — the overlay
paints last and its `PANEL_ALPHA = 190` panels would dim lens glyphs into looking absent.

**What this means for parcel size.** There is no funnel to tap, so a design that intercepts `Canvas::text`
would need to reach into four modules. But there **is a single point in time**: one contiguous block, one
thread, one `present_view`, all three surfaces finished by line 2226. **A snapshot published once, at the
bottom of that block, sees everything.** That is the seam, and it makes this a small parcel — one hook
site, plus a `lines()`-style accessor on each of the three surface owners.

---

## 6. Threading — the fact that makes this easy

**Hosted, the bus handlers run on the frontend's own main thread, synchronously, inside `bus.pump(&mut sys)`
at `main.rs:1970`.** `Host::pump` (`host.rs:455`) swaps the `System` into the `Engine`, drains
`EngineMsg::Call` from an mpsc receiver, calls `self.engine.dispatch(&method, &params)` **on this thread**
(`host.rs:486`), and swaps back. Connection threads only *send* requests and *receive* reply strings; the
architecture comment at `host.rs:26-33` states the invariant: *no socket is ever written from the thread
that owns the machine.*

Therefore:

* **No lock is needed.** A `&mut` field write from the render loop, read by a handler on the same thread.
* `Arc<Mutex<Overlay>>` would be the **wrong** shape and should be refused if proposed.
* Ordering is deterministic: the push happens at the end of iteration *N*'s present; the next `pump` is at
  the top of iteration *N+1*. **The served text therefore describes the frame that is actually on the
  glass** — never a frame being composed, never one that has not been shown.

Headless (`crates/oracle-aether/src/main.rs` → `Server::spawn`) is the other arrangement: the `System`
lives on a dedicated engine thread (`server.rs:426 engine_loop`) and **there is no frontend in the process
at all.** `oracle-aether` depends on `oracle-core` and `serde_json` only; the dependency arrow is
frontend → aether and never the reverse.

---

## 7. The seam — four options, priced

### 7.1 Widen `MachineInfo` — ✘ reject

**Cost:** small edit, three struct declarations to keep in step (`host.rs:543`, `bus.rs:39`,
`bus_stub.rs:25`). **Risk:** it is not a state slot. `set_machine_info` destructures and discards
(`host.rs:232`), and it is called exactly twice in the program's life. Making it per-frame means either
calling it 60×/s (allocating and moving a `SymbolTable` every frame — a real cost, and a real chance of
dropping symbols on a reload race) or splitting it, at which point you have written option 7.2 with a
misleading name. **Cannot perturb emulation** (it touches no `System` field), but it is the wrong shape
and the name would lie. Reject.

### 7.2 A host-owned presentation-state slot, pushed once per present — ✔ **RECOMMEND**

Mirror `Host::set_live_pads` exactly:

```
frontend loop (main.rs, end of the present block, after ov.draw)
   └─ bus.set_screen_text(snapshot)          // bus.rs  — one-line delegate
        └─ Host::set_screen_text(snapshot)   // host.rs — one-line delegate
             └─ Engine::set_screen_text(s)   // engine.rs — `self.screen = Some(s);`
                                             //   a plain field beside Engine::layers / Engine::live
   handler emulator/screen_text (&mut Engine) reads self.screen
```

**Cost:** four one-line delegates (`bus.rs`, `bus_stub.rs` no-op twin, `host.rs`, `engine.rs`), one
`Engine` field, one handler, one snapshot builder in the frontend, plus `lines()`-style accessors on
`Overlay`/`Palette`/lens models. Plus the contract work in §7.5, which is the real cost.

**Risk:** the snapshot allocates per present. Bound it (§8.4) and skip it when
`!host.has_clients()` — the same guard `Host::publish_capture` already uses for the far more expensive frame
memcpy (`host.rs:269`: *"a host uses this to skip per-frame work nobody would read"*).

**Perturbation: none, structurally.** The field lives on `Engine`, which is **outside `System`** — the
same placement `Engine::layers` uses and for the same stated reason (`engine.rs:4366-4373`): it cannot
enter `state_hash`/`memory_hash`, cannot be undone by `reset`/`reload_rom`/`restore`, and does not trip
§6's run-control rule. See §10.

**Why it wins:** it is the only option with three working precedents in this tree, it needs no lock, it
needs no new thread, and the answer it serves is *the text that was actually presented* rather than a
recomputation.

### 7.3 The frontend serves its own extra methods alongside the core set — ✘ reject, and refuse if proposed

**Not possible today, and the way to make it possible is a defect.** `METHODS` is a `const` slice of
`MethodSpec { name, handler: fn(&mut Engine, &Value) -> Result<Value, RpcError>, summary, params }`
(`engine.rs:215-234`); there is no vec, no trait object, no `register_method`. The frontend never sees the
table except to print its length (`bus.rs:124`).

Making it dynamic would let a frontend-only method **escape the schema harness**: `schema_conformance.rs`
and `params_closure.rs` both iterate `engine::METHODS`, so a method that is not in that table is served on
the wire with nothing checking its shape and nothing saying so. That is the exact failure mode
`UNCOVERED_METHODS` exists to make loud. Reject on those grounds, not on effort.

### 7.4 A `screenshot` + OCR pipeline in the caller — ✘ reject

`emulator/screenshot` already serves the presented picture, so a caller *could* OCR it today. Reject:
5×7 bitmap glyphs at variable integer scale, folded case, hollow boxes for unmapped characters, panels at
190/255 alpha over arbitrary game art. It would be less accurate than the strings we already hold, and it
would still miss the window title (§3.4) entirely. Named here because it is the status quo and someone
will propose keeping it.

### 7.5 The gate you cannot route around

`crates/oracle-aether/tests/contract/bus-protocol.schema.json` is **vendored verbatim** from
`empyrean/contract/schema/bus-protocol.schema.json`, and
`schema_conformance.rs::the_vendored_schema_is_byte_identical_to_the_upstream_contract` enforces it. You
**cannot hand-edit the vendored copy.**

Adding a method to `METHODS` without an upstream fragment turns two tests red, by design:

* `schema_conformance.rs:~240` — `const UNCOVERED_METHODS: &[&str] = &[];`, pinned **empty**. Its own
  failure message says: *"add it to UNCOVERED_METHODS deliberately, or (better) get a fragment into the
  contract schema first."*
* `params_closure.rs:85-104` — panics: *"{name} is advertised but has no params fragment."*

Also in the blast radius: `handshake.rs:60-64` (advertised set ≡ `METHODS`), `handshake.rs:105-140`
(`methodSummaries` key set ≡ `methods`), `methods.rs:57`, `checkpoints.rs:109`, `schema_dryrun.rs:295`,
`mcp_tool_sweep.rs`, and `tests/common::sweep_params` needs a case for the new name. **There is no
hard-coded method count anywhere** — the repo deliberately removed them
(`schema_conformance.rs:10-14`) — so nothing breaks on the number alone.

**Order of operations: contract first, then implementation.** That is §8's rule and this repo's own
record of it working (CR-10, CR-13). This recon does not own that ruling.

---

## 8. The wire shape — derived from what the two callers need

### 8.1 Does the contract already cover it?

**No.** The schema's 18 `$defs` are `hex`, `symbolName`, `hash64`, `hash32`, `handle`, `boundedList`,
`id`, `stamp`, `replyFields`, `request`, `successResponse`, `errorResponse`, `notification`,
`errorObject`, `watchStamp`, `decoderLayout`, `decodedSlot`, `objectAtBody`. There is **no** fragment for
a list of text lines or `{kind, text}` records — the raw schema has zero hits for `"lines"`, `"text"`,
`"entries"` or `"overlay"`. `"kind"` appears only inside `pixel_attribution`'s `owner` enum.

The one thing that **is** reusable is **`$defs/boundedList`** — `{items, total, returned, truncated,
cursor?}`, whose own description says *"item shape is the owning method's business; the envelope around it
is not."* §2.4 requires it for any list that is a field of a result. **The new method's array must be
wrapped in it.** The item shape is genuinely new and needs a CR.

### 8.2 Proposed method and shape

`emulator/screen_text`, no params, a pure read (no timeline mutation, so §6's run-control rule does not
reach it):

```jsonc
{
  "surfaces": {                     // $defs/boundedList
    "items": [
      { "surface": "windowTitle",  "text": "Oracle — frame 12720 [PAUSED]",
        "rendered": "Oracle — frame 12720 [PAUSED]", "truncated": false, "unrenderable": [] },
      { "surface": "statusLine",   "text": "VOL 8/10 AETHER ON AUDIO VA2 4:3 320X224 F12720",
        "rendered": "VOL 8/10 AETHER ON AUDIO VA2 4:3 320X2", "truncated": true, "unrenderable": [] },
      { "surface": "pausedBanner", "text": "PAUSED", "rendered": "PAUSED",
        "truncated": false, "unrenderable": [] },
      { "surface": "toast",        "text": "watch cleared — no longer recording writes",
        "rendered": "watch cleared — no longer recording",
        "truncated": true,  "unrenderable": ["—"] }
    ],
    "total": 4, "returned": 4, "truncated": false
  },
  "displayed": true,
  "presentedFrame": 12720,          // §10 — NAMED as the window's counter, not the bus's
  "picture": { "w": 960, "h": 672 },
  "fontScale": 3
}
```

**Every field justified against a caller need, not against what is easy to serve:**

| Field | Which caller needs it, and why |
|---|---|
| `surface` | Both. A caller must be able to ask "is the machine paused" without string-matching every line. Enum, closed: `windowTitle`, `statusLine`, `layerBadge`, `pausedBanner`, `toast`, `paletteTitle`, `paletteQuery`, `paletteRow`, `lensCpu`, `lensProfile`, `lensWatch`, `lensHover`. Closed rather than free-form so adding a surface is a contract edit, not a silent drift. |
| `text` | **Caller A.** The message, whole. The thing the sibling lane is actually asking the owner to read out. |
| `rendered` | **Caller B.** What survived `fit`. Without it, `F-TOAST-TRUNCATES` is invisible to every automated check, forever. |
| `truncated` | Both. **REQUIRED even when false**, per §2.4(a) / §2.3's rule that absence and `false` must not both mean "you have everything". A caller must never have to compare two strings to learn that one was cut. |
| `unrenderable` | Both. §8.3. |
| `displayed` | §9. |
| `presentedFrame` | Both, and it is a landmine — §10. |
| `picture`, `fontScale` | Caller B. Truncation is a function of these; without them a failing check cannot say *at what window size*. `docs/2026-08-29-window-runtime-checks.md` §6 records "one window size" as a stated coverage gap. |

**Order is Z order** (title, then lens, then palette, then overlay — §5), and it should be stated in the
fragment. A caller reproducing the screen needs it; a caller that only filters by `surface` ignores it
for free.

### 8.3 ⚑ The honest field: `unrenderable`

Neither `text` nor `rendered` can show a missing-glyph box (§3.5). `font::glyph` is the authority and it
is right there. So the snapshot builder computes, per surface, the characters in `rendered` for which
`font::glyph(c.to_ascii_uppercase())` is `None` — the exact predicate the drawing path uses.

This costs one pass over an already-short string and it converts a class of defect that is *structurally
invisible to a string-level readout* into a first-class wire fact. It also makes `F-FONT-BACKTICK` and
`F-FONT-EMDASH` assertable by a test that never renders a pixel. **This is the field that makes the
feature honest rather than merely useful**, and it is the one thing here that a parity-shaped design
would have omitted.

### 8.4 Bounds

Toasts cap at 5 (`MAX_TOASTS`). The palette is the unbounded one — ~20 rows on an empty query, more with
a long ROM listing (`Picker.items` is one entry per file in a directory). Cap the item count, set
`boundedList.truncated` honestly, and **do not** emit a `cursor`: §2.4(b) forbids a handle on a method
that accepts no continuation. Cap each string too — a `format!("OPEN ROM - {}", dir.display())` on a deep
path is unbounded.

---

## 9. What a caller gets when the player is not running — **refuse, loudly**

**The decisive fact: "a window showing no text" is a reachable, ordinary state.** With F3 off, no lenses,
no toasts, the palette closed, default layers, and the machine running, `Overlay::draw` draws **zero
characters**. That is the default launch. So an empty `items: []` cannot also mean "there is no window",
because it already means something else, and a caller could never tell them apart.

**Therefore:**

* **Headless (`oracle-aether`, or a frontend built `--no-default-features`, or before the first present):**
  refuse. `-32005 INVALID_STATE` (`rpc.rs:38`) with a `reason` naming the condition —
  `noDisplay` / *"this server has no window; screen text exists only in a hosted player"*. An error is
  the loudest available answer and it cannot be mistaken for content.
* **Windowed, nothing on screen:** succeed, `items: []`, `displayed: true`. Now the two are
  distinguishable and both are true.

`displayed` is carried on the success reply anyway, and that is not redundant with the error: it lets a
caller that got a *success* confirm it is talking to a window without parsing an error, and it is the
field a future non-window presenter would set false. **REQUIRED even when true**, same §2.3 reason as
`truncated`.

**Rider worth pricing in the same CR:** add `display: boolean` to `emulator/status`'s result, so a caller
can branch *before* provoking an error. `status` already carries process-identity facts (`romPath`,
`symbolsPath`, `romBytes`). One extra boolean, and it turns "probe by failing" into "ask". Cheap; the CR
is already open.

---

## 10. ⚑ `F-WINDOW-BUS-FRAME-OFFBYONE` — **diagnosed. It is not a convention difference. Do not join these numbers.**

The registry entry (`docs/OVERSEER.md:1280`, `docs/2026-08-29-window-runtime-checks.md:240`) guesses *"a
completed-vs-presenting convention difference would explain it entirely and would not be a bug."*
**That guess is wrong.** Source at this revision, no runtime needed:

**They are two different quantities, computed by two different mechanisms, that are never reconciled.**

* **Bus `frame` / `frameToken`** — `Engine::frame()`, `crates/oracle-aether/src/engine.rs:2223`:
  `self.sys.scheduler().now() / MCLK_PER_FRAME`. **Derived from the emulated master clock**, every time it
  is asked. The handler's own comment at `engine.rs:2237` says: *"Deliberately the **emulated** frame
  index, not a UI counter. The sibling's `frame_token` is a UI counter, which forced hand-rolled
  realignment three separate ways (recon §5 C2)."*
* **Window `F …`** — `let mut frame: u64 = 0;` at `crates/oracle-frontend/src/main.rs:1143`. **An
  independent software counter**, `frame += 1` at `main.rs:1929` after *every* `run_frames_with_sink(1, …)`
  call, plus `frame += pumped.frames_advanced` at `main.rs:1975` (after a bus-driven advance).

**The repo already made this ruling once, on the bus side, and named the UI counter as the thing it was
refusing. The window is that counter.**

### 10.1 The mechanism of the observed +1

`main.rs:1929`'s `frame += 1` fires after every run **whether or not the run completed a frame.** A
breakpoint stop ends the run mid-frame (`StopReason::SinkRequested`), so:

* `scheduler().now()` sits *inside* frame 12719 → `now() / MCLK_PER_FRAME` = **12719**.
* the loop counted one more iteration → **12720**.

Exactly the observed pair. And it **does not self-correct**: `System::run_frames_with_sink`
(`crates/oracle-core/src/system.rs:930-939`) snaps `frame_boundary_mclk` back to the last whole boundary
crossed on an early stop, so the next iteration finishes *that same* frame and increments again. **Every
breakpoint stop adds a permanent +1 to the window's counter relative to the bus's.** It accumulates.

### 10.2 Two more divergence sources, both worse

* **A save-state load rewinds the clock and not the counter, deliberately and in writing.**
  `main.rs:1571-1574` prints `"state: loaded slot {n} from {path} (frame counter continues at {frame})"`. The
  restored `System` carries the saved `mclk`, so the bus's frame jumps backwards while the window's does
  not. **Unbounded divergence, in the other direction.**
* **Bus-driven runs are floor-counted.** `Engine::frames_advanced` (`engine.rs:1873-1881`) returns
  `(now - mclk_before) / MCLK_PER_FRAME` on any early stop — correctly exact for its own reply, but the
  frontend adds it to a counter that also counted the partial frame. Drift both ways.

Reset and ROM reload *do* agree: `main.rs:1592`/`:1770` zero the counter and `System::reset` does
`*self = Self::new(seed)` (`system.rs:494`), zeroing the scheduler.

### 10.3 The ruling this forces on the feature

**The wire field must be named `presentedFrame`, documented as the window's own presentation counter, and
its description must state that it is NOT `frameToken` and must not be joined to it.** Serving it as
`frame` would put a colliding key on the envelope anyway (§2.2's stamp overwrites same-named keys — the
same trap `watch_stamp_json` was nested to avoid, `engine.rs:6376`).

**Do not "fix" the window's counter as part of this parcel.** Two defensible repairs exist —
(a) `frame += 1` only when `blit_capture` returned a completed frame, (b) drop the local counter and read
`sys.scheduler().now() / MCLK_PER_FRAME` — and (b) is almost certainly right, since it makes the window
and the bus the same number by construction. But it changes what the status line and the window title say,
which is a visible behaviour change, and it deserves its own parcel and its own ruling. **What this recon
delivers is that the lead is closed as a real accumulating defect, not a benign convention** — and that a
consumer joining these two numbers would be wrong, silently, more often the longer the session runs.

**Registry action:** `F-WINDOW-BUS-FRAME-OFFBYONE`'s "probably a convention difference; unverified" text
is now disproven and should be replaced, not merely annotated.

---

## 11. ⚑ `F-TOAST-TRUNCATES` — the decision, made explicitly

**Serve both strings. Do not pick one.**

The brief is right that this is a design decision and that picking silently would be wrong. The reason it
resolves to "both" rather than to a choice is §0's two callers, and one asymmetry:

* Serving only `text` (source) means **no automated check can ever see `F-TOAST-TRUNCATES` or any defect
  like it.** The class of bug is "the glass says less than the model does" — a reader of the model is
  blind to it by construction. The repo's own 2026-08-29 bar says *"assert on the **whole** rendered
  string."*
* Serving only `rendered` means Caller A is handed `OPEN ROM: CANNOT READ /TMP/…/LOCKED (PE` and told that
  is the message. It is not; it is the message's shadow.

**Cost of both: near zero.** `fit` returns a borrowed prefix of the source, computed at the same moment
from the same `avail`/`px` the drawing code uses. The snapshot builder calls the **same function**, not a
restatement of its arithmetic — `overlay::status_text_avail` @495 exists precisely because a restated copy
*"would agree with itself while drifting from the drawing code, which is the shape this repo keeps paying
for."* Follow that: the builder must call `fit` and `status_text_avail`, never re-derive them.

`truncated` is required-even-when-false so a caller learns of a cut without comparing strings, and
`unrenderable` (§8.3) covers the part neither string can show.

**Note for the implementer:** this parcel does not fix the truncation. It makes it *visible*. Fixing it
(an ellipsis, a reason-first field order, a wrap) is `F-TOAST-TRUNCATES`'s own parcel — and the readout
built here is the instrument that would prove the fix.

---

## 12. How this preserves the safety property

**The property, first-hand.** `Vdp::render_scanline` — `crates/oracle-core/src/render.rs:1965`:

```rust
pub fn render_scanline(&mut self, line: u16) -> LineReport
```

It is the one render that takes `&mut self` and commits the sticky sprite-overflow / sprite-collision
status latches and the R10 dot-overflow carry (`commit_scanline_sprites`). Its doc, `render.rs:1961`:
*"It takes no `LayerMask`, and it deliberately has no masked twin… there is no argument to thread, so no
caller can reach the sprite-latch commit through a mask."* Every masked function —
`render_line_masked` @1517, `render_line_report_masked` @1713, `pixel_attribution_masked` @1853,
`resolve_line_masked` @1460 — is `&self` and returns a fresh value. `LayerMask` (`render.rs:157`) is
*"a **parameter**, never a field: no `Vdp` and no `System` holds one, so it is in no snapshot, no
`state_hash`, and nothing a reset or a restore can carry or drop."*

**How §7.2 preserves it — three independent reasons, any one sufficient:**

1. **It never renders.** The snapshot is `String`s already built by the frontend for its own drawing. It
   calls no `Vdp` method, masked or otherwise. There is no path from `emulator/screen_text` to
   `render_scanline`.
2. **It never touches `System`.** The field sits on `Engine`, beside `Engine::layers` (`engine.rs:803`)
   and `Engine::live`, which are outside `System` for exactly this reason (`engine.rs:4366-4373`). It
   therefore cannot enter `state_hash` or `memory_hash`, cannot be moved by `reset`/`reload_rom`/
   `restore`, and is invisible to every frozen currency.
3. **It is a pure read on the wire.** No timeline mutation, so §6's run-control state rule does not reach
   it and it needs neither `require_paused` nor a `machineRunning` refusal.

**The design that would break it, named so it is refused if proposed:** a handler that *composes the
overlay on demand* — "tell me what the screen would say" — computed inside `dispatch`. It would need
`area` and `px` from the window, would duplicate the layout arithmetic §11 just argued against
duplicating, and would put display composition on the code path that holds `&mut Engine` with the real
`System` swapped in. Publish-a-snapshot has none of those properties. **Reject compose-on-demand.**

---

## 13. What I could NOT establish, and the instrument that would settle it

This repo's rule: an unavailable instrument retires the instrument, never the question. Each item names a
second instrument.

| # | Open question | First instrument (unavailable here) | Second instrument |
|---|---|---|---|
| 1 | ⟨RUNTIME⟩ Does the +1 of §10.1 **persist** after resume, as the source says it must? Only the pre-stop and post-resume *bus* numbers were recorded on 2026-08-29; the window's `F` after resume was never read. | A windowed player + a bus client — forbidden to this seat (standing invariant 2; and the corrected xvfb recipe still lands on the owner's desktop if `WAYLAND_DISPLAY` leaks). | **Source, settled:** `system.rs:930-939` snaps the anchor back on `SinkRequested`, and `main.rs:1929` increments unconditionally, so persistence follows from the two together. A *cheaper confirmation than runtime*: a unit test over `System::run_frames_with_sink` with a stop sink, asserting `now()/MCLK_PER_FRAME` lags the call count by exactly the number of early stops. That is a headless test and it is the one to write. |
| 2 | ⟨RUNTIME⟩ Do `F-FONT-BACKTICK` / `F-FONT-EMDASH` actually render as hollow boxes on glass? | Screenshot of a live window. | **Source, settled:** `font.rs:30-99` has no `` ` `` and no U+2014; `font.rs:175` substitutes `MISSING` for `glyph(..) == None`. A test asserting `font::glyph('`').is_none()` **and** that the literal at `main.rs:1207` contains a char with no glyph closes it with no pixels at all — and, unlike `font.rs:238`, it reads the real literal instead of restating it. |
| 3 | Exact per-present cost of building the snapshot. | `cargo bench` / a timed run. | Bound it by construction instead of measuring: gate on `has_clients()` (the precedent `Host::publish_capture` sets at `host.rs:269`), cap items and string lengths (§8.4). A cost you have bounded does not need to be measured to be safe. |
| 4 | Whether `boundedList` is the right envelope, or whether the contract wants a bespoke one. | The contract's ruling — **not ours to make** (§7.5). | Raise it as the CR with §8.2's shape and §8's justification table attached, and let the contract rule each key on its merits. That is what CR-13 did and it is why coverage went the right way round. |
| 5 | Whether aeon actually wants `text`, `rendered`, or both. | Ask the requesting lane. | Not blocking: §11's argument stands on our *own* checks needing `rendered`, independently of what aeon wants. Serving both cannot be wrong for either. Worth asking anyway before the CR closes. |

---

## 14. Implementation checklist for whoever takes the parcel

**Gated on §7.5 — do not start at step 4.**

1. Raise the CR against `empyrean/contract/` with §8.2's shape, the justification table, §9's refusal, and
   §10.3's naming ruling. Include the optional `emulator/status.display` rider.
2. Contract rules; fragment lands upstream.
3. Re-vendor `crates/oracle-aether/tests/contract/bus-protocol.schema.json` and update
   `tests/contract/PROVENANCE.md` (blob + sha256).
4. `Engine`: one field beside `layers` (`engine.rs:803`), one setter, one handler, one `MethodSpec` row.
5. `Host::set_screen_text` (`host.rs`, beside `set_live_pads` @315).
6. `Bus::set_screen_text` in **both** `crates/oracle-frontend/src/bus.rs` **and**
   `crates/oracle-frontend/src/bus_stub.rs` (no-op) — identical signatures, or
   `--no-default-features` stops compiling.
7. Frontend: a `screen_text()` snapshot builder. **It must call `overlay::fit`,
   `overlay::status_text`, `overlay::status_text_avail` and `font::glyph` — never restate their
   arithmetic.** Add `lines()`-style accessors to `Overlay`, `Palette`, and the lens models.
8. Hook site: `main.rs` immediately after `ov.draw(...)` (~2226), inside the present block, gated on
   `has_clients()`.
9. Tests: the whole-rendered-string assertion the 2026-08-29 bar demands; the `displayed: false` refusal
   against `oracle-aether`; the empty-but-displayed case; `unrenderable` non-empty for `main.rs:1207`'s
   backtick; and §13 row 1's early-stop counter test.
10. Update `F-WINDOW-BUS-FRAME-OFFBYONE` in `docs/OVERSEER.md` — its stated hypothesis is disproven
    (§10). Register `F-FONT-BACKTICK` and `F-FONT-EMDASH` (§3.5).
