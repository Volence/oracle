//! **`emulator/pacing`** — §11.42 (CR-S), the server's own obligations.
//!
//! The row is served only by a process that puts frames on a screen, so this file has to run **two
//! servers**: a headless one, which must not advertise it, and a presenting one, which must. Everything
//! else here is about the two conditional shapes M3 rules on — *unmeasured is not a zero* — and those are
//! asserted against a **served reply**, never against a hand-built document. The contract's own ten
//! vectors already prove the fragment refuses the wrong documents; `schema_conformance` runs them. What
//! no vector can prove is that this server never *emits* one, which is what is checked below.
//!
//! # The headless negative is a suite obligation, and the ruling says so
//!
//! §11.42 M4: *"The headless-advertises negative is a suite obligation on the server, not a schema
//! vector."* A schema describes a document; "which binary is answering" is not a property of one. So
//! [`a_headless_server_does_not_advertise_pacing`] is the discharge of that clause, and its positive
//! control is not decoration: an enumeration that found **nothing** would report the same "pacing is
//! absent" as a correct one, and the CR that ordered this row was itself filed after exactly that
//! mistake (*"the first run of this check produced NONE from an extraction that had found zero
//! methods"*).
//!
//! # §11.42 S2 was adopted after this file landed, and it is asserted here
//!
//! S2 (*"`fps.value` at zero presented frames is a MEASURED 0.0, not an unmeasured arm"*) was added to
//! the contract on 2026-09-06, **after** the serve below shipped. This server already conformed — by
//! construction, `counted * 1000 / window_ms` is `0.0` at `counted == 0` — but conformance nobody
//! asserts is a defect waiting for its first refactor, and this repo has already spent ten days with a
//! handler, its comment and its test mutually consistent and all three wrong. The S1 half was asserted
//! at landing; [`zero_presented_frames_is_a_measured_zero_fps_with_its_window`] is the S2 half.
//!
//! # Red-first evidence, 2026-09-06 — every row below was planted against and went red
//!
//! Eleven mutations were applied to the *shipped* source, one at a time, each proven on disk with
//! `git diff --stat` before the run and each restored from the committed baseline afterwards. **The
//! parameter was varied rather than repeated**, because this repo's strongest guard was once found to
//! have a hole reachable in one mutation and only a varied sweep found it. M3 in particular was mutated
//! in **both** directions, which is what the ruling itself asks for: dropping the flag and adding a
//! count beside it are different defects and would be caught by different assertions.
//!
//! | mutation applied to the server | the row that went red |
//! |---|---|
//! | drop `unmeasured` entirely | `an_absent_audio_device_is_stated_and_never_counted` |
//! | emit `underruns: 0` beside `unmeasured: true` | `an_absent_audio_device_is_stated_and_never_counted` |
//! | drop `underruns` on a MEASURED device | `a_measured_zero_is_served_as_a_zero` |
//! | emit `p50`/`p99` as `0.0` at zero samples | `nothing_sampled_is_zero_samples_and_no_percentiles` |
//! | emit `p50` without `p99` | `the_reply_is_the_measurement_the_process_published` |
//! | `presents_frames` defaults to `true` | `a_headless_server_does_not_advertise_pacing` |
//! | hide the row from `initialize` but let it dispatch | `a_headless_server_does_not_advertise_pacing` |
//! | empty `PRESENTING_ONLY` | that row **and** `a_presenting_deployment_advertises_…` |
//! | declare `params: &["windowMs"]` | `a_client_cannot_choose_the_measurement_window` |
//! | serve `target_fps` as `presented` | `the_reply_is_the_measurement_the_process_published` |
//! | **break the enumeration this file reads** | `a_headless_server_does_not_advertise_pacing`, on its CONTROL |
//! | `let _ = self.advance(1)` inside the handler | `reading_the_pacing_moves_nothing` |
//!
//! The last one is the anti-vacuity row and the only one applied to *this file*: the `methods` key was
//! misspelled so the array came back empty. The negative assertion still held — an empty list contains
//! no `emulator/pacing` — and the run went red **on the positive control**, which is the whole reason
//! the control is asserted first. A positive control validates the SEARCH, not the question.
//!
//! Four more were run against the player's half (the derivation and the panel); their record is on
//! `oracle_player::pacing::Readout::of`'s parity test.
//!
//! # Red-first evidence for the S2 row, 2026-09-06 — and three of these the schema cannot see
//!
//! Five more mutations were run for [`zero_presented_frames_is_a_measured_zero_fps_with_its_window`],
//! recorded in their own table because **three of them validate clean against the vendored fragment**.
//! That is the answer to "why a suite row when the schema already requires both keys": a schema can
//! require a key, but it cannot know what the key must *be*.
//!
//! | mutation applied to the serialiser | schema verdict | what went red |
//! |---|---|---|
//! | drop `"value"` from the served `fps` | REJECTS | the row, on the validator |
//! | drop `"windowMs"` from the served `fps` | REJECTS | the row, on the validator |
//! | `"value": p.fps_value.max(1.0)` | **accepts** | the row ALONE, on `== 0.0` |
//! | `"unmeasured": p.presented == 0` beside the zero | **accepts** | the row ALONE, on the key count |
//! | `"windowMs": 1000` (nominal, not elapsed) | **accepts** | the row, on the window |
//!
//! Under each of the middle three the *other eleven rows in this file stayed green* and the fragment
//! raised nothing: a server could have fabricated a rate at zero presents, or flagged one `unmeasured`,
//! and every other guard in the repo would have certified it. The fourth is the defect S2 names in its
//! own words, and until this row existed nothing anywhere refused it.

#![cfg(unix)]

mod common;

use oracle_aether::engine::{
    EngineConfig, FrameTimes, PacingAudio, PacingFacts, METHODS, PRESENTING_ONLY,
};
use oracle_aether::host::{Host, HostConfig, MachineInfo};
use oracle_core::system::System;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

static SEQ: AtomicU32 = AtomicU32::new(0);

fn temp_socket(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!("ap-{tag}-{}-{n}.sock", std::process::id()))
}

/// A measurement with every field distinguishable from every other, so a serialiser that crossed two of
/// them could not pass. The numbers are **not** round and not equal to each other on purpose: `presented`
/// and `targetFps` and `windowMs` would all be plausible as `60`, and a test where they were could not
/// tell `presented` from `targetFps`.
fn a_measurement() -> PacingFacts {
    PacingFacts {
        presented: 18_121,
        fps_value: 59.94,
        fps_window_ms: 997,
        frame_time: FrameTimes::Sampled {
            samples: 123,
            p50_ms: 16.71,
            p99_ms: 33.42,
        },
        audio: PacingAudio::Measured { underruns: 7 },
        target_fps: 60,
    }
}

// ---------------------------------------------------------------------------------------------------
// A presenting deployment: the hosted arrangement with `presents_frames` set, over a real socket.
// Modelled on `tests/hosted.rs`'s player, cut down to what this row needs — it publishes a pacing
// measurement once per iteration and never draws anything, because nothing here reads pixels.
// ---------------------------------------------------------------------------------------------------

struct Presenter {
    socket: PathBuf,
    stop: Arc<AtomicBool>,
    iterations: Arc<AtomicU64>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Presenter {
    /// `facts` is what the loop publishes every iteration. `None` is a presenting process that has not
    /// measured anything yet — a real state, and the one arm of the refusal path.
    fn start(tag: &str, facts: Option<PacingFacts>) -> Self {
        let socket = temp_socket(tag);
        let stop = Arc::new(AtomicBool::new(false));
        let iterations = Arc::new(AtomicU64::new(0));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (t_socket, t_stop, t_iters) =
            (socket.clone(), Arc::clone(&stop), Arc::clone(&iterations));
        let thread = std::thread::Builder::new()
            .name("test-presenter".into())
            .spawn(move || {
                let mut sys = System::new(0x5EED);
                sys.load_rom(oracle_core::testrom::build());
                sys.reset();
                let mut host = Host::new(HostConfig {
                    // ⚑ The one line that makes this a presenting deployment. `oracle-player`'s
                    // `Bus::new` sets exactly this, beside `window_gestures`.
                    engine: EngineConfig {
                        presents_frames: true,
                        free_run_pace: None,
                        ..HostConfig::default().engine
                    },
                    ..HostConfig::default()
                });
                host.set_machine_info(MachineInfo {
                    rom_path: Some("testrom".to_string()),
                    ..MachineInfo::default()
                });
                host.serve(Some(t_socket)).expect("bind the pacing socket");
                ready_tx.send(()).ok();
                while !t_stop.load(Ordering::SeqCst) {
                    // Where the real loop publishes it: once per present, before the drain.
                    if let Some(f) = facts {
                        host.set_pacing(f);
                    }
                    host.pump(&mut sys);
                    t_iters.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(1));
                }
            })
            .expect("spawn the presenter");
        ready_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the presenter bound its socket");
        Self {
            socket,
            stop,
            iterations,
            thread: Some(thread),
        }
    }
}

impl Drop for Presenter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        let _ = std::fs::remove_file(&self.socket);
        let _ = self.iterations.load(Ordering::SeqCst);
    }
}

/// The smallest NDJSON client that can hold a conversation. Deliberately not shared with
/// `tests/common`'s: that one talks to a `Server`, and half of this file's subject is that the two
/// arrangements answer `initialize` differently.
struct Wire {
    r: BufReader<UnixStream>,
    w: UnixStream,
    id: i64,
}

impl Wire {
    fn connect(p: &Presenter) -> Self {
        let s = UnixStream::connect(&p.socket).expect("connect to the presenter");
        s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        Self {
            r: BufReader::new(s.try_clone().unwrap()),
            w: s,
            id: 0,
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        let line = json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params});
        writeln!(self.w, "{line}").expect("write the request");
        self.w.flush().unwrap();
        loop {
            let mut s = String::new();
            let n = self.r.read_line(&mut s).expect("read a reply");
            assert!(n > 0, "the presenter closed the connection");
            let v: Value = serde_json::from_str(&s).expect("a reply parses");
            // Skip notifications; they carry no `id`.
            if v.get("id") == Some(&json!(self.id)) {
                return v;
            }
        }
    }

    fn handshake(&mut self) -> Value {
        self.call("initialize", json!({"clientId": "pacing-test"}))["result"].clone()
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        let v = self.call(method, params);
        // The schema's verdict on the actual served line, not on a copy of it: this is the same
        // validator every reply in this suite goes through, and it is what ties the reply to the
        // fragment vendored in this commit.
        common::schema::check_incoming_strict(&v, Some(method)).unwrap_or_else(|e| {
            panic!("{method} reply violates the vendored fragment: {e:?}\n{v}")
        });
        assert!(
            v.get("error").is_none(),
            "{method} refused unexpectedly: {v}"
        );
        v["result"].clone()
    }

    fn err(&mut self, method: &str, params: Value) -> Value {
        let v = self.call(method, params);
        v["error"].clone()
    }
}

// ---------------------------------------------------------------------------------------------------
// M4 — advertisement is per deployment
// ---------------------------------------------------------------------------------------------------

/// **The negative §11.42 M4 puts on this suite: a headless server MUST NOT advertise `emulator/pacing`.**
///
/// Three claims, and the order matters.
///
/// 1. **The positive control first.** The enumeration is proven to see rows that *are* there before its
///    silence about one row is read as evidence. A `methods` array that failed to parse, or a handshake
///    that returned an error object, would otherwise produce the same "pacing is absent" as a correct
///    server — the exact confusion the CR behind this row was filed after making.
/// 2. `emulator/pacing` is not in the array.
/// 3. And it does not dispatch either. A server that hid the row and answered it anyway would satisfy
///    the letter of M4 while breaking D4, and would leave a client that guessed the name better off than
///    one that read the advertisement.
#[test]
fn a_headless_server_does_not_advertise_pacing() {
    let h = common::spawn("pacing-headless");
    let mut c = common::Client::connect(&h);
    let r = c.handshake(true);
    let methods: Vec<&str> = r["methods"]
        .as_array()
        .expect("`methods` is an array")
        .iter()
        .map(|v| v.as_str().expect("a method name is a string"))
        .collect();

    // (1) The control. These four are unconditional rows: if the enumeration cannot see them, it cannot
    // see anything, and its silence about `emulator/pacing` means nothing.
    for control in [
        "emulator/status",
        "emulator/registers",
        "emulator/screenshot",
        "emulator/screen_text",
    ] {
        assert!(
            methods.contains(&control),
            "the control row {control} is missing, so this enumeration proves nothing about what \
             else is absent: {methods:?}"
        );
    }

    // (2) The negative itself.
    assert!(
        !methods.contains(&"emulator/pacing"),
        "§11.42 M4: a headless server MUST NOT advertise `emulator/pacing` — it presents no frames, so \
         there is nothing for it to measure. Advertised: {methods:?}"
    );

    // (3) And the advertisement is not a curtain in front of a working handler.
    let e = c.err("emulator/pacing", json!({}));
    assert_eq!(
        e["code"],
        json!(-32601),
        "a row this deployment does not advertise must not dispatch either (D4): {e}"
    );
}

/// **The other half of the same fact, without which the negative above could pass on a deleted row.**
///
/// A presenting deployment advertises it; and the *difference* between the two advertised sets is
/// exactly [`PRESENTING_ONLY`] — derived by differencing two live handshakes rather than by naming a
/// number, so a second deployment-dependent row added tomorrow lands here rather than going quiet.
#[test]
fn a_presenting_deployment_advertises_exactly_the_presenting_only_rows_more() {
    let headless = {
        let h = common::spawn("pacing-diff-headless");
        let mut c = common::Client::connect(&h);
        let r = c.handshake(true);
        r["methods"]
            .as_array()
            .expect("array")
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<std::collections::BTreeSet<String>>()
    };
    let p = Presenter::start("pacing-diff", Some(a_measurement()));
    let mut w = Wire::connect(&p);
    let r = w.handshake();
    let presenting: std::collections::BTreeSet<String> = r["methods"]
        .as_array()
        .expect("array")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    assert!(
        presenting.contains("emulator/pacing"),
        "a process that presents frames MUST advertise the row it can answer"
    );
    let extra: Vec<&String> = presenting.difference(&headless).collect();
    let missing: Vec<&String> = headless.difference(&presenting).collect();
    assert!(
        missing.is_empty(),
        "a presenting deployment dropped rows the headless one serves: {missing:?}"
    );
    let want: Vec<String> = PRESENTING_ONLY.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(
        extra.into_iter().cloned().collect::<Vec<String>>(),
        want,
        "the difference between the two deployments must be exactly `PRESENTING_ONLY`"
    );
    // `methodSummaries` is derived from the same registry (§2.1 rule 2), so it moves with it or the two
    // have grown a second producer.
    assert!(
        r["methodSummaries"]["emulator/pacing"].is_string(),
        "the summary map must carry every advertised row: {}",
        r["methodSummaries"]
    );
}

/// The row is in the dispatch table exactly once and takes no params — the source-level half, so a serve
/// that was reverted while this file still passed on a stale binary cannot go unnoticed.
#[test]
fn the_row_is_declared_read_only_and_paramless() {
    let rows: Vec<&oracle_aether::engine::MethodSpec> = METHODS
        .iter()
        .filter(|m| m.name == "emulator/pacing")
        .collect();
    assert_eq!(rows.len(), 1, "one row, named once");
    assert!(
        rows[0].params.is_empty(),
        "§11.42 M1/M2: the row takes no params at all — the measurement window is the server's"
    );
}

// ---------------------------------------------------------------------------------------------------
// M1 / M2 — the quantities, and the window that is part of the fps
// ---------------------------------------------------------------------------------------------------

/// **The reply is the published measurement, field for field**, and it validates against the fragment
/// vendored in this commit (`Wire::ok` runs the validator on every reply here).
///
/// Every expected value is read back out of the `PacingFacts` the presenter published rather than
/// retyped, so this cannot drift into asserting a constant that no longer matches what was measured.
#[test]
fn the_reply_is_the_measurement_the_process_published() {
    let facts = a_measurement();
    let p = Presenter::start("pacing-reply", Some(facts));
    let mut w = Wire::connect(&p);
    w.handshake();
    let r = w.ok("emulator/pacing", json!({}));

    assert_eq!(r["presented"], json!(facts.presented));
    assert_eq!(r["fps"]["value"], json!(facts.fps_value));
    assert_eq!(r["fps"]["windowMs"], json!(facts.fps_window_ms));
    assert_eq!(r["targetFps"], json!(facts.target_fps));
    let FrameTimes::Sampled {
        samples,
        p50_ms,
        p99_ms,
    } = facts.frame_time
    else {
        panic!("the fixture is a sampled measurement");
    };
    assert_eq!(r["frameTimeMs"]["samples"], json!(samples));
    assert_eq!(r["frameTimeMs"]["p50"], json!(p50_ms));
    assert_eq!(r["frameTimeMs"]["p99"], json!(p99_ms));
    let PacingAudio::Measured { underruns } = facts.audio else {
        panic!("the fixture has a device");
    };
    assert_eq!(r["audio"]["unmeasured"], json!(false));
    assert_eq!(r["audio"]["underruns"], json!(underruns));

    // ⚑ **M1's own sentence, as an assertion.** `presented` is frames put on the glass; `frameToken` is
    // the emulated frame index. The fixture's presented count is one this machine's frame counter cannot
    // reach in the milliseconds this test runs, so the two being equal would mean the serialiser had
    // reached for the wrong quantity.
    let status = w.ok("emulator/status", json!({}));
    assert_ne!(
        r["presented"], status["frameToken"],
        "§11.42 M1: `presented` is the window's DRAWS and is never `status.frameToken`"
    );
}

/// **§11.42 M2 / §8 item 22: the measurement window is the SERVER's, so a client that thinks it chose
/// one is refused** — by name, and before any handler runs.
#[test]
fn a_client_cannot_choose_the_measurement_window() {
    let p = Presenter::start("pacing-window", Some(a_measurement()));
    let mut w = Wire::connect(&p);
    w.handshake();
    let e = w.err("emulator/pacing", json!({"windowMs": 1000}));
    assert_eq!(e["code"], json!(-32602), "{e}");
    assert_eq!(
        e["data"]["unknownParams"],
        json!(["windowMs"]),
        "the refusal must name the key, so a client learns what it got wrong: {e}"
    );
    // The row this deployment DOES serve is still swept by the closure probe the headless sweep in
    // `params_closure.rs` cannot reach — same probe, other side of M4.
    let e = w.err("emulator/pacing", json!({"notARealParamName": 1}));
    assert_eq!(e["code"], json!(-32602), "{e}");
}

// ---------------------------------------------------------------------------------------------------
// M3 — unmeasured is expressible and is NEVER a zero
// ---------------------------------------------------------------------------------------------------

/// **No audio device: `unmeasured: true`, and NO count beside it.**
///
/// Both halves are asserted, because they are the two ways this can go wrong and they fail differently:
/// dropping `unmeasured` makes "no device" and "zero underruns" the same document, and emitting an
/// `underruns` beside `unmeasured: true` is a fabricated zero wearing a flag.
#[test]
fn an_absent_audio_device_is_stated_and_never_counted() {
    let facts = PacingFacts {
        audio: PacingAudio::Unmeasured,
        ..a_measurement()
    };
    let p = Presenter::start("pacing-noaudio", Some(facts));
    let mut w = Wire::connect(&p);
    w.handshake();
    let r = w.ok("emulator/pacing", json!({}));
    assert_eq!(r["audio"]["unmeasured"], json!(true));
    assert!(
        r["audio"].get("underruns").is_none(),
        "§11.42 M3: a count beside `unmeasured: true` is a zero nobody measured: {}",
        r["audio"]
    );
    // …and the audio object carries nothing else either, so the absence above is the whole object's
    // shape rather than one key that happened to be missing.
    assert_eq!(
        r["audio"].as_object().expect("an object").len(),
        1,
        "the unmeasured audio object is exactly `{{unmeasured: true}}`: {}",
        r["audio"]
    );
}

/// **A real zero is served as a real zero** — the control for the row above. Without it, a serialiser
/// that dropped `underruns` unconditionally would pass that test and be catastrophically wrong.
#[test]
fn a_measured_zero_is_served_as_a_zero() {
    let facts = PacingFacts {
        audio: PacingAudio::Measured { underruns: 0 },
        ..a_measurement()
    };
    let p = Presenter::start("pacing-zero", Some(facts));
    let mut w = Wire::connect(&p);
    w.handshake();
    let r = w.ok("emulator/pacing", json!({}));
    assert_eq!(r["audio"]["unmeasured"], json!(false));
    assert_eq!(
        r["audio"]["underruns"],
        json!(0),
        "a device that has underrun zero times is a MEASUREMENT, and it is served: {}",
        r["audio"]
    );
}

/// **Nothing sampled yet is `samples: 0` with the percentiles ABSENT**, never a `0.0` median.
///
/// This is the row the estimator's own signature protects (`stats::nearest_rank` answers `None` on an
/// empty slice), and the one a naive implementation gets wrong for free, because almost every percentile
/// helper ever written returns zero for an empty input.
#[test]
fn nothing_sampled_is_zero_samples_and_no_percentiles() {
    let facts = PacingFacts {
        frame_time: FrameTimes::Unsampled,
        ..a_measurement()
    };
    let p = Presenter::start("pacing-unsampled", Some(facts));
    let mut w = Wire::connect(&p);
    w.handshake();
    let r = w.ok("emulator/pacing", json!({}));
    assert_eq!(r["frameTimeMs"]["samples"], json!(0));
    for k in ["p50", "p99"] {
        assert!(
            r["frameTimeMs"].get(k).is_none(),
            "§11.42 M3: `{k}` over zero samples is fabricated: {}",
            r["frameTimeMs"]
        );
    }
    assert_eq!(
        r["frameTimeMs"].as_object().expect("an object").len(),
        1,
        "the unsampled frame-time object is exactly `{{samples: 0}}`: {}",
        r["frameTimeMs"]
    );
}

/// **§11.42 S2: zero presented frames is a MEASURED `fps.value` of `0.0`, with the window beside it.**
///
/// The other side of the clause `nothing_sampled_is_zero_samples_and_no_percentiles` covers, and the
/// distinction is the whole of it. A percentile over zero samples is *fabricated*, so it is absent; a
/// **rate** over an elapsed window is not, because zero frames in 997 ms is a fact the window makes true.
/// `unmeasured` belongs to a quantity with no instrument — audio with no device, the row above — never
/// to a sample that happened to come back empty. So the honest reply here is a real `0.0`, and treating
/// this state the way the audio row is treated would be the *opposite* defect from the one M3 guards.
///
/// Both halves are asserted because they fail differently, and a test that checked only the zero would
/// pass against a reply that dropped the window:
///
/// 1. `value` is **present**, a number, and `0.0` — not absent, not `null`, not a flag.
/// 2. `windowMs` is present beside it, carrying the fixture's own span. An fps without its window is
///    invalid rather than merely unhelpful (M2), and the window travelling with the figure is what makes
///    the zero readable as a measurement instead of a placeholder.
/// 3. And the object is **exactly** those two keys, so an `unmeasured: true` bolted on beside a `0.0`
///    cannot slip through. The schema cannot catch that one — `fps` sets no
///    `unevaluatedProperties: false` — nor can it catch a fabricated non-zero, since `value` is only
///    `{type: number, minimum: 0}` there. Which is why this row exists at all: the fragment can say the
///    key must be present, but only a suite obligation can say *what it must be* at zero presents.
///
/// The fixture is the coherent state rather than a spliced one: nothing presented, so nothing timed
/// either. That lets the reply show both idioms of emptiness at once — `fps.value: 0.0` **with** its
/// window, and `frameTimeMs: {samples: 0}` with nothing else — which is the clause's closing sentence.
#[test]
fn zero_presented_frames_is_a_measured_zero_fps_with_its_window() {
    let facts = PacingFacts {
        presented: 0,
        fps_value: 0.0,
        frame_time: FrameTimes::Unsampled,
        ..a_measurement()
    };
    let p = Presenter::start("pacing-zerofps", Some(facts));
    let mut w = Wire::connect(&p);
    w.handshake();
    let r = w.ok("emulator/pacing", json!({}));

    assert_eq!(
        r["presented"],
        json!(0),
        "the fixture presented nothing: {r}"
    );
    // (1) A number, and the number is zero. `json!(0.0)` will not compare equal to `Value::Null`, so
    // this rejects a null arm as well as a wrong figure; `is_number` states the absent case by name
    // rather than leaving it to a confusing `Null == 0.0` failure message.
    assert!(
        r["fps"].get("value").is_some_and(Value::is_number),
        "§11.42 S2: `fps.value` at zero presented frames is a MEASURED 0.0, not an absent or \
         unmeasured arm — zero frames over an elapsed window is a fact about the window. Got: {}",
        r["fps"]
    );
    assert_eq!(
        r["fps"]["value"],
        json!(0.0),
        "§11.42 S2: the honest figure at zero presents is exactly 0.0: {}",
        r["fps"]
    );
    // (2) The window beside it, read back off the fixture rather than retyped.
    assert_eq!(
        r["fps"]["windowMs"],
        json!(facts.fps_window_ms),
        "§11.42 S2/M2: the window travels WITH the figure — an fps without its window is invalid, \
         and the zero is only readable as a measurement because the span is there: {}",
        r["fps"]
    );
    // (3) …and nothing else, so an `unmeasured` flag cannot be smuggled in beside the zero.
    assert_eq!(
        r["fps"].as_object().expect("an object").len(),
        2,
        "§11.42 S2: the fps object is exactly `{{value, windowMs}}`; `unmeasured` is M3's word for a \
         quantity with no instrument and has no place on a rate: {}",
        r["fps"]
    );

    // The clause's closing sentence: the same moment, the other field, the other idiom. Empty samples
    // ARE absent percentiles — which is what makes the presence of `fps.value` above a decision.
    assert_eq!(r["frameTimeMs"]["samples"], json!(0));
    assert!(
        r["frameTimeMs"].get("p50").is_none() && r["frameTimeMs"].get("p99").is_none(),
        "§11.42 S2: at the same moment, a percentile over zero samples IS omitted — the two fields \
         differ on purpose, and a server that treated them alike got one of them wrong: {}",
        r["frameTimeMs"]
    );
}

/// **The governor being OFF is `targetFps: 0`, present**, not a missing key — §11.42 M1's reason for
/// making the field REQUIRED, which is that a paced 60 and a free-running 60 are different facts.
#[test]
fn a_governor_that_is_off_is_a_zero_target_and_not_an_absent_one() {
    let facts = PacingFacts {
        target_fps: 0,
        ..a_measurement()
    };
    let p = Presenter::start("pacing-unpaced", Some(facts));
    let mut w = Wire::connect(&p);
    w.handshake();
    let r = w.ok("emulator/pacing", json!({}));
    assert_eq!(
        r["targetFps"],
        json!(0),
        "`--target-fps 0` is reported as 0, and the key is present: {r}"
    );
}

/// **"Read-only" is the §6 row's own word, so it is asserted rather than assumed.**
///
/// Two observables are read before and after two `emulator/pacing` calls, because neither alone is
/// enough: `frameToken` would miss a row that poked memory without advancing a frame, and
/// `emulator/state_hash` **covers VDP state only** — its own reply says so in a caveat, in these words:
/// *"they say nothing about the CPU, work RAM, the Z80, SRAM or audio"*. So this is a check over the
/// VDP plus the frame position, stated at that scope rather than described as "the whole machine",
/// which is what an earlier draft of this comment claimed and the reply's own caveat disproved.
///
/// ⚑ This row is deliberately **not** covered by `handshake.rs`'s per-row frame-advance ceiling, which
/// sweeps the names a server actually advertised — and on that headless server this one is not among
/// them. A deployment-dependent row leaves a deployment-dependent hole in every sweep written against
/// the other deployment, which is the cost of §11.42 M4 and is paid here rather than left implicit.
#[test]
fn reading_the_pacing_moves_nothing() {
    let p = Presenter::start("pacing-readonly", Some(a_measurement()));
    let mut w = Wire::connect(&p);
    w.handshake();
    let before = w.ok("emulator/state_hash", json!({}));
    let token_before = w.ok("emulator/status", json!({}))["frameToken"].clone();
    w.ok("emulator/pacing", json!({}));
    w.ok("emulator/pacing", json!({}));
    let after = w.ok("emulator/state_hash", json!({}));
    for k in ["vram", "cram", "vsram", "regs", "combined"] {
        assert_eq!(
            before[k], after[k],
            "`emulator/pacing` moved `{k}`; the §6 row says read-only"
        );
    }
    assert_eq!(
        token_before,
        w.ok("emulator/status", json!({}))["frameToken"],
        "`emulator/pacing` advanced the machine; the §6 row says read-only"
    );
    // The control: the hashes are real readings and not a constant the server made up, so the loop
    // above compared something. The shape is the fragment's own — `0x` plus 16 hex of FNV-1a — and the
    // four parts must not all be the same value, which a stub returning one filler would produce.
    for k in ["vram", "cram", "vsram", "regs"] {
        let h = before[k].as_str().unwrap_or_default();
        assert!(
            h.starts_with("0x") && h.len() == 18,
            "`{k}` is {h:?}, not the documented 16-hex fingerprint, so the loop proves nothing"
        );
    }
    assert!(
        before["vram"] != before["cram"],
        "every fingerprint came back the same value, which is a stub rather than a reading: {before}"
    );
}

/// **A presenting process that has not measured yet REFUSES rather than inventing a measurement.**
///
/// It is the one state where the row is advertised and cannot be answered, and the honest reply is an
/// error. Serving a struct of zeroes here would be M3's defect at the level of the whole object.
#[test]
fn a_presenter_that_has_published_nothing_refuses() {
    let p = Presenter::start("pacing-nothing", None);
    let mut w = Wire::connect(&p);
    w.handshake();
    let e = w.err("emulator/pacing", json!({}));
    assert_eq!(e["code"], json!(-32005), "{e}");
    assert_eq!(e["data"]["reason"], json!("noPacing"), "{e}");
}
