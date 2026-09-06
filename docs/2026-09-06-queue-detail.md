# Queue detail, oracle — full row text as of 2026-09-06

Written when `docs/lane-status.json`'s queue was pruned from 34 rows to the rows a fresh session
could start or that wait on the owner, per `contract/LANE_STATUS.md` rule 7 (*a row states its state;
the history lives in a docs file and the row points at it by id*).

**Every row that was on the board is below, verbatim.** Pruning the board is not closing the item:
a row that left the status file is still live work, and this file is where its statement of the
problem now lives. Kept rows are marked so the two halves can be told apart.

## `FIXTURE-REPIN` — KEPT ON THE BOARD

*state* `blocked` · *size* `M` · *blockedBy* `your word, and a condition that cannot be met as written` · *project* `ORACLE-DEFAULT`

Your call. Refreshing our frozen copies of the game's symbol files to a current build would close the sixteen blind spots the new check merely names -- but it changes what every fixture-based test here is measured against. Also newly blocked on a fact: those files exist nowhere in your game's history, only as build output, so 'pin them to a named version' is not directly possible. Cleanest fix is asking aeon to publish them alongside a version.

## `F-PLANE-RASTER-UNGUARDED` — KEPT ON THE BOARD

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEBUG-UI`

Found by accident while I was checking someone else's work, and low priority on purpose: nothing in the tests notices if the plane picture draws the wrong tile in every cell. I transposed the picture's own lookup and all thirty-one of its tests stayed green. It matters little because a scrambled picture is obvious the moment you look at it -- which is exactly why it was never guarded -- but the reading you click for IS guarded now, so the two halves of that panel are held to different standards.

## `F-PLANE-CELL-TOOL` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEFAULT`

Recorded as a decision rather than an omission: another program can ask what is at a dot on the screen, but not what is in a map cell the screen is not currently showing. The window can answer it; the wire cannot. Nothing needs it yet.

## `CR-LOOKUP-PHASE` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEFAULT`

Deliberately NOT built with tonight's fix, recorded so it is a decision rather than an omission: other programs still cannot ask over the wire where a piece of code runs versus where it is stored. Tonight's fix restores correct answers for every existing user without changing anything they agreed to, which was the whole fault. Publishing the pair is a separate, additive change request, and the code is already shaped for it.

## `F-PHASE-COMMENT-SAYS-EIGHT` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEFAULT`

One imprecise word in a comment, left rather than dirty a tree mid-run: it says eight lines where it means eight lines counted as damage, being a heading, a count and six rows. The two passages beside it are already exact. Rides the next parcel that touches the file.

## `STYLE-NUMBER-BAKEOFF` — KEPT ON THE BOARD

*state* `blocked` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEBUG-UI`

Built and in your next build: both ways of showing a headline number, side by side at the top of the Pacing tab, labelled, with real values. Say A or B and the other one is deleted. Correcting myself: I told you one option was accent coloured and it is not, the number takes a health colour either way. The choice is bare versus boxed and nothing about colour.

## `DATA-DISPLAY-AUDIT` — KEPT ON THE BOARD

*state* `blocked` · *size* `L` · *blockedBy* `None` · *project* `ORACLE-DEBUG-UI`

Page written and the Pacing tab rebuilt as the worked example, both published. Four questions need your eyes when you relaunch, and one of them decides the other panels: should a headline number be plain coloured text or a small bordered tile. Also whether the Objects columns read wrong because of the gap or the header size, whether the new level meter is informative or just decoration, and whether three big numbers fit the narrow right column. The rest of the panels follow your answers.

## `F-AUDIT-PAGE-CONTRADICTS-ITSELF` — KEPT ON THE BOARD

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEBUG-UI`

The page that guides the rest of your panel work disagrees with itself about what colour a headline number is, and I copied the wrong half onto your decision card before catching it. Its reference table says one thing on line 134 and its own open-questions list says another on line 467. The shipped code agrees with the table. Fix is an appended correction rather than an edit, since the page's line numbers are stamped to a revision and editing them would break the thing that makes it trustworthy.

## `RELAUNCH-2` — KEPT ON THE BOARD

*state* `blocked` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEBUG-UI`

Rebuilt with your icon, the working panels menu, the Objects table, and Tab restarting the game again; the engine team has the build. Three things to look at when you relaunch that nobody can check without eyes: the whole window changed colour, the fixed-width text is smaller, and Tab no longer steps between fields. One thing needs your say-so first: on Wayland the icon needs a desktop entry installed into your home folder. It writes five icon files and two new entries, and REPLACES one file already there from an earlier install. Nobody has run it. On X11 none of that is needed.

## `F-INITIAL-DOCK-MISNOMER` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEBUG-UI`

Found while moving your picker, harmless today and misleading tomorrow: the code that builds the default window arrangement names one of its panes after the wrong thing, so the comments around it describe a layout that is not the one being built. Nothing is broken and every panel docks correctly. Left alone deliberately because renaming it touches a line that several tests measure, so it should ride along with the next layout change rather than be done for its own sake.

## `F-PANELS-INVISIBLE-TO-SCREEN-TEXT` — KEPT ON THE BOARD

*state* `open` · *size* `M` · *blockedBy* `None` · *project* `ORACLE-DEBUG-UI`

The reason I had to ask you about the click instead of reading it myself: the window can report what its top bar says to a connected program, but not what any panel says. Fixing that turns every future panel complaint from 'ask him' into something I can read off the wire.

## `F-THREE-MASKED-RENDERERS` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEBUG-UI`

Three separate pieces of code now draw the same layer-hidden picture, in three different parts of the program. They agree today and nothing checks that they must. Not urgent; it becomes urgent the moment one is edited.

## `SCHEMA-DRIFT-NIGHTLY` — KEPT ON THE BOARD

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `None`

Our contract check stopped reading the contract team's live folder today; the honest cost is that an ordinary run no longer notices them moving on. Fix is a nightly that reports drift without blocking anyone. Nothing new to build.

## `VRAM-WRITE-RULE-CR` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEFAULT`

Video memory can be written while the game runs and the other three memory spaces cannot. Our emulator is following the written rules exactly; the rules are what look wrong. Needs raising with the contract team; gates nothing.

## `ORACLE-SANITY-WEEKLY` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `None`

A weekly check that the emulator itself has not drifted, since other teams' tests run inside it and a regression there could make one of their checks pass for no reason. Existing suites only; an afternoon at most.

## `S1-DIALECT-FIXTURE` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `None`

Five checks on how we read symbol listings now sit out, because the file they compared against lives in another project and is not version-controlled. Freezing a small excerpt as our own would give the coverage back with no outside dependency.

## `Y-SIGN-COVERAGE` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEFAULT`

Nothing on the emulator's own test side checks that a negative vertical position survives being read. Correct today; a regression would pass unnoticed.

## `F-ACCEPT-TABLE-RAWSTRING` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `None`

Residual from the acceptance-table hardening: the gap in how the table reads raw strings is real, but four probes could not produce a dangerous case from it. Either prove the shape or close it as theoretical.

## `ACCEPT-16` — moved here

*state* `open` · *size* `L` · *blockedBy* `None` · *project* `ORACLE-DEFAULT`

Eight advertised-but-unserved debugger methods, all correctly waiting: three whose consumer asked us not to pre-build, two needing a part of the emulator not compiled in, one the consumer does not want, two parked as no-ops.

## `WIKI-SPIKE` — moved here

*state* `open` · *size* `L` · *blockedBy* `None` · *project* `None`

The wiki-emulator spike you approved, on Opus, with the report-the-wall condition.

## `OVERLAY-STATE` — moved here

*state* `blocked` · *size* `M` · *blockedBy* `runs only when no player window is open on the machine` · *project* `None`

Serves 56 methods, verified by spawning the binary. It reads the emulator's own chrome, not game pixels, so it does not end their eyeball requests. Left: never run against a real window.

## `PEER-CLAIMS-SWEEP` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `None`

Our notes make claims about other teams' code that nothing can ever check — one expired tonight and sent me to the wrong task. Now also: any note that says 'oracle' as if it were ONE emulator, in prose about measurements. There are two, they disagree by a line, and a measurement whose instrument is unnamed is unreadable. The boot doc is already clean; the risk is the dated notes a peer might cite.

## `REGISTER-WHICH-SERVER-SWEEP` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEFAULT`

Our own notes sometimes describe a fault without saying WHICH of the two emulators has it. Three such entries turned out to be about the old one and cost this session real work. Scope now narrowed by a check I ran: the startup file is CLEAN (three mentions, all correctly qualified), so the exposure is the dated design notes a teammate might cite - which makes this cheaper than it looked.

## `ERROR-SURFACE-GATE` — KEPT ON THE BOARD

*state* `open` · *size* `M` · *blockedBy* `None` · *project* `ORACLE-DEFAULT`

Our contract checks read replies only, so they are blind to every duty to REFUSE a bad request. This was a proposal for weeks; today it has a real defect it would have caught, found by hand. Do not re-argue it from principles - it has an observation now.

## `F-STATUS-CAVEAT-NOT-ON-STRIP` — moved here

*state* `open` · *size* `M` · *blockedBy* `None` · *project* `ORACLE-DEFAULT`

Booked properly now: the always-visible line at the top of the debug window does not carry the stale-names warning, and it is the surface that most needs it. Lands late in the window rebuild, with the identity slice.

## `F-FRONTEND-NO-STATUS` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEFAULT`

The game window never asks for status at all, so no staleness warning can reach it by any route. Smaller than it sounds, and worth knowing before someone assumes the warning is everywhere.

## `F-MCP-SYMBOL-FRESHNESS-NO-BANNER` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEFAULT`

The tool wrapper shouts when the GAME is out of date but leaves the out-of-date-names warning buried in the reply body. Asymmetric, and it lives in the old repo we are replacing, so it may die with the cutover instead.

## `F-BOOTSTRAP-STANZA-UNPINNED` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEFAULT`

The one paragraph our startup file is allowed to copy from the shared rulebook is richer than the copy the rulebook permits, and it names no version of what it was copied from - so if the shared rule changes, nothing here can notice. Found by the agent doing the split, against this lane. Real but slow, harming no reader today, so it is booked rather than fixed while the no-ceremony cut holds.

## `F-VSYNC-NEVER-MEASURED` — moved here

*state* `blocked` · *size* `S` · *blockedBy* `deliberately deferred to the point the old window would be deleted` · *project* `ORACLE-DEBUG-UI`

The frame-rate number the window rebuild's finish line rests on was never measured - the run that was authorised for it ran unthrottled and answered a different question. Ruled to ask you for one short run at the point the old window would be deleted, not before, so it costs you one interruption instead of two.

## `CR-SAVESTATE-BY-METHOD` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEFAULT`

Booked from tonight, not built: a way for a program to save and load a machine state to a FILE, so a paused game can be handed to another copy of the emulator. The whole floor investigation stopped at step one because the only way to save is a key on your keyboard, and the in-memory version cannot leave the process that made it.

## `F-TOYSTORY-SILENT` — KEPT ON THE BOARD

*state* `open` · *size* `M` · *blockedBy* `None` · *project* `ORACLE-DEFAULT`

Toy Story is silent in our emulator and I measured why it is not the obvious causes: both windows have working, unmuted audio, and the game's sound program IS loaded and running. So the gap is inside our sound emulation. First question is cheap and yours: does the OLD emulator play it with sound?

## `CR-FM-PSG-READBACK` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEFAULT`

The instrument that would answer the silence in one call, the same shape as the video-chip readback that landed tonight: let a program ask what the sound chips have been told. Twice today an investigation stopped at exactly this kind of boundary.

## `F-REGISTER-PEEK-MISSES-MIDFRAME` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEFAULT`

Found tonight and it limits the tool we just built: reading the video chip gives its state at one instant, so a setting the game changes DURING the drawing of a frame is invisible to it. Your floor reads 'whole plane' while visibly doing something per-line, which is most likely exactly that.

## `F-FIXTURES-TEACH-RINGS-ARE-OBJECTS` — moved here

*state* `open` · *size* `S` · *blockedBy* `None` · *project* `ORACLE-DEBUG-UI`

Booked rather than fixed, because the fixtures themselves are sound and it is the MODEL they teach that is wrong. Several picker tests use a made-up object called a ring in their sample lists, which is good test design in itself (one of them is deliberately the object that publishes nothing, so the nothing-to-show message has something to be about). But rings turned out not to be objects at all, and anyone reading those tests would reasonably conclude they are. That is exactly the wrong belief my own instructions carried into this work.
