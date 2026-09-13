//! **SPIKE — H22 design parcel (2026-09-13). NOT PROPOSED FOR MERGE.**
//!
//! A measurement harness for `docs/2026-09-13-h22-decode-design.md`, compiled only under the `h22-spike`
//! cargo feature (default OFF, so no normal build, test run or CI leg sees any of this). It puts a
//! runtime-selectable front end in front of [`decode_dispatch`] so one binary can time, on the SAME
//! workload: the cascade as it is, an eager `(opcode, S)` table with a cascade fallback for the keys that
//! are not a pure function of that pair, a double decode (the add-one estimate of decode's own cost), and a
//! recorder (the real-load opcode histogram plus a sample of the exact register files decode was handed).
//!
//! The mode comes from `H22_SPIKE_MODE` at first use (so a `cargo test --release` of the replay
//! playthroughs can be timed per mode with no code change) and can be overridden with [`set_mode`].
//!
//! Lives as a child module of `decode` only so it can reach the private `decode_dispatch`.

use super::decode_dispatch;
use crate::m68000::microop::{MicroOp, MicroState, Size, MAX_OPS};
use crate::m68000::registers::{Registers, SR_SUPERVISOR};
use std::cell::{Cell, RefCell};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{LazyLock, OnceLock};

/// The cascade exactly as `main` runs it (the hook returns `None`).
pub const MODE_CASCADE: u8 = 0;
/// Eager table for the statically-pure keys, the cascade for the rest.
pub const MODE_TABLE: u8 = 1;
/// Decode twice and keep the second: the extra wall-clock is one decode's cost under real load.
pub const MODE_DOUBLE: u8 = 2;
/// The cascade, plus a histogram of every key and a strided sample of the register files.
pub const MODE_RECORD: u8 = 3;
/// The table, asserting on every pure-key lookup that it equals the cascade on the LIVE registers.
pub const MODE_TABLE_CHECKED: u8 = 4;
/// Lazy memo (option B): one `OnceLock` per key, filled from the canonical registers on first use.
pub const MODE_LAZY: u8 = 5;

/// Option B's storage: a pointer-sized `OnceLock` per key; only touched keys allocate a recipe.
pub static LAZY: LazyLock<Vec<OnceLock<Option<Box<MicroState>>>>> =
    LazyLock::new(|| (0..KEYS).map(|_| OnceLock::new()).collect());

/// How many keys the lazy memo has filled (a `None` for an impure key counts as filled).
pub fn lazy_filled() -> usize {
    LAZY.iter().filter(|c| c.get().is_some()).count()
}

/// Option B's lookup: the memoized recipe for a pure key (filling it on first use), `None` for an impure one.
#[inline]
pub fn lazy_lookup(regs: &Registers) -> Option<MicroState> {
    let k = key_of(regs);
    LAZY[k]
        .get_or_init(|| {
            let op = k as u16;
            if static_impure(op) {
                None
            } else {
                Some(Box::new(decode_canonical(op, k >> 16 == 1)))
            }
        })
        .as_deref()
        .cloned()
}

static MODE: LazyLock<AtomicU8> = LazyLock::new(|| {
    let m = std::env::var("H22_SPIKE_MODE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(MODE_CASCADE);
    AtomicU8::new(m)
});

/// Select the front-end mode for every subsequent `decode` in the process.
pub fn set_mode(m: u8) {
    MODE.store(m, Ordering::Relaxed);
}

/// The current front-end mode.
pub fn mode() -> u8 {
    MODE.load(Ordering::Relaxed)
}

static FAST_FILL: LazyLock<AtomicBool> = LazyLock::new(|| {
    AtomicBool::new(std::env::var("H22_SPIKE_FASTFILL").is_ok_and(|v| v == "1"))
});

/// Make `RecipeBuf::new` copy its filler from [`FILLER`] instead of the `[X; MAX_OPS]` repeat expression.
pub fn set_fast_fill(on: bool) {
    FAST_FILL.store(on, Ordering::Relaxed);
}

/// Whether `RecipeBuf::new` copies its filler from [`FILLER`].
#[inline]
pub fn fast_fill() -> bool {
    FAST_FILL.load(Ordering::Relaxed)
}

/// The inert filler every recipe pads with, as a `static` so a copy of it is a block move from memory.
pub static FILLER: [MicroOp; MAX_OPS] = [MicroOp::Internal { cycles: 0 }; MAX_OPS];

/// The dispatch's builder for a handful of hot opcodes, called DIRECTLY (no arm walk), latched like
/// `decode`. `None` for any opcode not listed. `decode(regs) - builder_direct(regs)` is the walk's cost.
pub fn builder_direct(regs: &Registers) -> Option<MicroState> {
    let op = regs.prefetch[0];
    let mut st = match op {
        0x3200 => super::move_recipe(op, Size::Word),
        0x4A38 => super::tst_recipe(op, Size::Byte),
        0x67FA | 0x6604 => super::bcc_recipe(op, regs),
        0x51C8 | 0x51C9 => super::dbcc_recipe(op, regs),
        0x4E75 => super::rts_recipe(),
        0x7001 => super::moveq_recipe(op),
        _ => return None,
    };
    st.set_opcode(op);
    Some(st)
}

/// The `(S, opcode)` key space: 2 × 65 536.
pub const KEYS: usize = 1 << 17;

/// The table key for a register file: the S bit above the 16-bit opcode.
#[inline]
pub fn key_of(regs: &Registers) -> usize {
    (usize::from(regs.sr & SR_SUPERVISOR != 0) << 16) | usize::from(regs.prefetch[0])
}

/// A **conservative** static mirror of the dispatch arms whose builders are handed `regs` and read it (the
/// eight builders taking `&Registers` in `decode.rs`, minus `movep_recipe`, whose `_regs` is unused). A
/// superset is harmless (the key merely falls back to the cascade); a MISSING opcode would put a wrong
/// recipe in the table, which is what the probe in the example exists to catch.
pub fn static_impure(op: u16) -> bool {
    let mode = (op >> 3) & 7;
    // BCHG/BCLR/BSET dynamic `Dn,Dn` — the decode-time `pos >= 16` idle reads D[(op>>9)&7].
    (matches!(op & 0xF1C0, 0x0140 | 0x0180 | 0x01C0) && mode == 0)
        // BCHG/BCLR/BSET static `#n,Dn` — the same idle reads the bit number from prefetch[1].
        || (matches!(op & 0xFFC0, 0x0840 | 0x0880 | 0x08C0) && mode == 0)
        // MOVEM (and the EXT encodings sharing the mask) — the register list is prefetch[1].
        || op & 0xFB80 == 0x4880
        // Bcc / BRA (not BSR) — taken/not-taken resolved against the live CCR.
        || (op >> 12 == 0x6 && (op >> 8) & 0xF != 1)
        // Scc and DBcc — the live CCR (DBcc also the live Dn.w counter).
        || op & 0xF0C0 == 0x50C0
        // TRAPV — the live V bit.
        || op == 0x4E76
        // Register shift/rotate with a Dn count — the idle reads D[(op>>9)&7] & 63.
        || (op & 0xF000 == 0xE000 && (op >> 6) & 3 != 3 && op & 0x20 != 0)
}

/// The dispatch's own privilege predicate (the gate in front of the cascade), exposed for the S-key question.
pub fn is_privileged(op: u16) -> bool {
    super::is_privileged_opcode(op)
}

/// The register file a table entry is built from: everything zero except the opcode and the S bit.
pub fn canonical_regs(op: u16, supervisor: bool) -> Registers {
    Registers {
        d: [0; 8],
        a: [0; 7],
        usp: 0,
        ssp: 0,
        pc: 0,
        sr: if supervisor { 0x2700 } else { 0x0000 },
        prefetch: [op, 0],
    }
}

/// `decode_dispatch` on [`canonical_regs`] — the recipe BEFORE the opcode latch.
pub fn dispatch_canonical(op: u16, supervisor: bool) -> MicroState {
    decode_dispatch(&canonical_regs(op, supervisor))
}

/// `decode` on [`canonical_regs`] — exactly what a table entry holds.
pub fn decode_canonical(op: u16, supervisor: bool) -> MicroState {
    let mut st = dispatch_canonical(op, supervisor);
    st.set_opcode(op);
    st
}

/// The eager table: `Some(recipe)` for every statically-pure key, `None` (cascade) for the rest.
pub static TABLE: LazyLock<Vec<Option<MicroState>>> = LazyLock::new(|| {
    (0..KEYS)
        .map(|k| {
            let op = k as u16;
            if static_impure(op) {
                None
            } else {
                Some(decode_canonical(op, k >> 16 == 1))
            }
        })
        .collect()
});

/// Build the table now (idempotent); returns how many keys it holds a recipe for.
pub fn force_table() -> usize {
    LazyLock::force(&TABLE).iter().filter(|e| e.is_some()).count()
}

// ---------------------------------------------------------------------------------------------------------
// The dependence probe: which register-file fields change the recipe, per key, found by perturbing each field
// and comparing the recipe — i.e. by what TOUCHES the data, not by what the source appears to read.
// ---------------------------------------------------------------------------------------------------------

/// Names of the probed fields, in bit order of [`probe_key`]'s mask (bit 31 = a joint-only dependence).
pub const FIELD_NAMES: [&str; NFIELDS] = [
    "d0", "d1", "d2", "d3", "d4", "d5", "d6", "d7", "a0", "a1", "a2", "a3", "a4", "a5", "a6", "usp", "ssp",
    "pc", "prefetch1", "C", "V", "Z", "N", "X", "T", "I2-I0", "sr-unimpl",
];
/// Number of probed fields.
pub const NFIELDS: usize = 27;
/// Bit set in a probe mask when the three base register files decode differently but no single-field
/// perturbation explains it (a dependence only on a combination of fields).
pub const JOINT_BIT: u32 = 1 << 31;

const V32: [u32; 24] = [
    0,
    1,
    2,
    7,
    8,
    0xF,
    0x10,
    0x11,
    0x1F,
    0x20,
    0x3F,
    0x40,
    0xFF,
    0x100,
    0x7FFF,
    0x8000,
    0xFFFF,
    0x1_0000,
    0x1234_5679,
    0x7FFF_FFFF,
    0x8000_0000,
    0x00FF_FFFE,
    0xFFFF_FFFE,
    0xFFFF_FFFF,
];

fn values_for(field: usize) -> &'static [u32] {
    match field {
        0..=18 => &V32,
        19..=24 => &[0], // a single-bit toggle; the value is ignored
        25 => &[0, 1, 2, 3, 4, 5, 6, 7],
        _ => &[0x4000, 0x1000, 0x0800, 0x0080, 0x0040, 0x0020, 0x58E0],
    }
}

fn perturb(base: &Registers, field: usize, v: u32) -> Registers {
    let mut r = base.clone();
    match field {
        0..=7 => r.d[field] = v,
        8..=14 => r.a[field - 8] = v,
        15 => r.usp = v,
        16 => r.ssp = v,
        17 => r.pc = v,
        18 => r.prefetch[1] = v as u16,
        19 => r.sr ^= 0x0001,
        20 => r.sr ^= 0x0002,
        21 => r.sr ^= 0x0004,
        22 => r.sr ^= 0x0008,
        23 => r.sr ^= 0x0010,
        24 => r.sr ^= 0x8000,
        25 => r.sr = (r.sr & !0x0700) | (((v & 7) as u16) << 8),
        _ => r.sr ^= (v as u16) & 0x58E0,
    }
    r
}

/// Three base register files per key — all-zero, all-ones, and mixed — so a dependence that only shows from
/// a non-zero starting point (a condition code that needs another bit set, a counter that is only zero from
/// one side) is still reached by a single-field perturbation from at least one of them.
fn bases(op: u16, supervisor: bool) -> [Registers; 3] {
    let s = if supervisor { SR_SUPERVISOR } else { 0 };
    let b0 = canonical_regs(op, supervisor);
    let b1 = Registers {
        d: [u32::MAX; 8],
        a: [u32::MAX; 7],
        usp: 0x00FF_FFFE,
        ssp: 0x00FF_FFFE,
        pc: 0x00FF_FFFE,
        sr: s | 0x071F,
        prefetch: [op, 0xFFFF],
    };
    let mut b2 = canonical_regs(op, supervisor);
    for (i, d) in b2.d.iter_mut().enumerate() {
        *d = 0x8000_0010 + i as u32;
    }
    for (i, a) in b2.a.iter_mut().enumerate() {
        *a = 0x0012_3450 + 2 * i as u32;
    }
    b2.usp = 0x0000_1000;
    b2.ssp = 0x0000_2000;
    b2.pc = 0x0000_0C00;
    b2.sr = s | 0x0305;
    b2.prefetch[1] = 0x8001;
    [b0, b1, b2]
}

/// The set of fields (a bitmask over [`FIELD_NAMES`], plus [`JOINT_BIT`]) whose value changes the recipe
/// `decode_dispatch` builds for this `(opcode, S)` key. `0` = the key is a pure function of `(opcode, S)` as
/// far as this directed probe can see.
pub fn probe_key(op: u16, supervisor: bool) -> u32 {
    let bases = bases(op, supervisor);
    let mut dep = 0u32;
    for base in &bases {
        let want = decode_dispatch(base);
        for f in 0..NFIELDS {
            if dep & (1 << f) != 0 {
                continue;
            }
            for &v in values_for(f) {
                if decode_dispatch(&perturb(base, f, v)) != want {
                    dep |= 1 << f;
                    break;
                }
            }
        }
    }
    if dep == 0 {
        let first = decode_dispatch(&bases[0]);
        if bases[1..].iter().any(|b| decode_dispatch(b) != first) {
            dep |= JOINT_BIT;
        }
    }
    dep
}

// ---------------------------------------------------------------------------------------------------------
// The recorder.
// ---------------------------------------------------------------------------------------------------------

/// What [`MODE_RECORD`] collected: a count per key, the total, and every `stride`-th register file.
pub struct Record {
    /// Decode calls per `(S, opcode)` key.
    pub hist: Vec<u64>,
    /// Every `stride`-th register file decode was handed, up to `cap`.
    pub samples: Vec<Registers>,
    /// Total decode calls recorded.
    pub calls: u64,
    stride: u64,
    cap: usize,
}

impl Record {
    fn empty(stride: u64, cap: usize) -> Self {
        Self {
            hist: vec![0; KEYS],
            samples: Vec::new(),
            calls: 0,
            stride: stride.max(1),
            cap,
        }
    }
}

thread_local! {
    static REC: RefCell<Record> = RefCell::new(Record::empty(1, 0));
    static CHECKED: Cell<u64> = const { Cell::new(0) };
}

/// Start a fresh recording on this thread.
pub fn reset_record(stride: u64, cap: usize) {
    REC.with(|r| *r.borrow_mut() = Record::empty(stride, cap));
}

/// Take this thread's recording.
pub fn take_record() -> Record {
    REC.with(|r| std::mem::replace(&mut *r.borrow_mut(), Record::empty(1, 0)))
}

/// How many table lookups [`MODE_TABLE_CHECKED`] has verified against the cascade on this thread (and reset).
pub fn take_checked() -> u64 {
    CHECKED.with(|c| c.replace(0))
}

fn record(regs: &Registers) {
    REC.with(|r| {
        let mut r = r.borrow_mut();
        let k = key_of(regs);
        r.hist[k] += 1;
        if r.calls % r.stride == 0 && r.samples.len() < r.cap {
            r.samples.push(regs.clone());
        }
        r.calls += 1;
        // A running summary on stderr every 2^22 calls, so a harness this spike cannot reach into (the
        // replay playthroughs, run with `--nocapture`) still reports its opcode mix.
        if r.calls % (1 << 22) == 0 {
            let impure: u64 = (0..KEYS)
                .filter(|&k| static_impure(k as u16))
                .map(|k| r.hist[k])
                .sum();
            let mut top: Vec<(usize, u64)> =
                r.hist.iter().copied().enumerate().filter(|x| x.1 > 0).collect();
            top.sort_by(|a, b| b.1.cmp(&a.1));
            let pct = |c: u64| 100.0 * c as f64 / r.calls as f64;
            let shown: Vec<String> = top
                .iter()
                .take(16)
                .map(|(k, c)| format!("{:#06x}:{:.2}%", *k as u16, pct(*c)))
                .collect();
            eprintln!(
                "H22-REC calls={} static_impure={:.2}% touched={} top={shown:?}",
                r.calls,
                pct(impure),
                top.len()
            );
        }
    });
}

/// The hook `decode` calls first under the feature: `Some(recipe)` short-circuits the cascade.
#[inline]
#[cfg_attr(feature = "h22-fixed", allow(dead_code))]
pub(super) fn front(regs: &Registers) -> Option<MicroState> {
    match mode() {
        MODE_CASCADE => None,
        MODE_TABLE => TABLE[key_of(regs)].clone(),
        MODE_LAZY => lazy_lookup(regs),
        MODE_DOUBLE => {
            black_box(decode_dispatch(black_box(regs)));
            None
        }
        MODE_RECORD => {
            record(regs);
            None
        }
        MODE_TABLE_CHECKED => {
            let got = TABLE[key_of(regs)].clone()?;
            let mut want = decode_dispatch(regs);
            want.set_opcode(regs.prefetch[0]);
            assert_eq!(
                got,
                want,
                "H22 spike: table != cascade for opcode {:#06x} S={} on the live registers",
                regs.prefetch[0],
                regs.supervisor()
            );
            CHECKED.with(|c| c.set(c.get() + 1));
            Some(got)
        }
        _ => None,
    }
}
