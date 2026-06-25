//! Effective-address machinery — a decode-time builder that emits, per EA mode, the shared sub-sequence
//! of micro-ops that fetches (source) or addresses (destination) an operand.
//!
//! The hybrid design (`docs/plans/2026-06-25-m68000-ea-machinery.md`): a [`RecipeBuf`] stages a recipe in
//! a fixed `[MicroOp; MAX_OPS]` array (no `Vec` — the recipe stays a `Copy`-friendly, bounded template),
//! and [`ea_src`]/[`ea_dst`] push the per-mode bus steps interleaved with the opcode's [`MicroOp::Alu`] at
//! the right [`AluPlacement`]. The placement is load-bearing: it pins *when* the ALU samples the prefetch
//! queue relative to the refills that shift it (e.g. `#imm` must read `prefetch[1]` **before** the two
//! refills that shift the immediate out — [`AluPlacement::First`]).
//!
//! C1 re-expresses the already-covered modes (`Dn` / `(An)` / `#imm` as a source, `Dn,(An)` as a
//! destination) through this builder; a regression test pins the builder's output byte-for-byte against
//! the literal recipes it replaces. Later commits add the remaining modes here, not in `decode`.

use super::microop::{MicroOp, MicroState, Operand, MAX_OPS};

/// Where an opcode's [`MicroOp::Alu`] sits relative to the prefetch refill(s) the EA emits.
///
/// The 68000 overlaps the operand combine with prefetch; *which* refill it straddles fixes which words
/// are live in the queue when the ALU samples it. Verified against the SST stream (see the plan's
/// source-read table):
/// - [`AluPlacement::First`] — `#imm`: the ALU reads the immediate from `prefetch[1]` **before** the two
///   refills shift it out.
/// - [`AluPlacement::AfterPrefetch`] — `Dn`/`An` direct: no operand read; the single refill happens, then
///   the ALU combines a register.
/// - [`AluPlacement::Last`] — memory operand: the operand is READ, the final refill happens, then the ALU
///   combines the just-read scratch value (the read is always the second-to-last bus event).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AluPlacement {
    /// ALU first, then the prefetch refill(s) (`#imm`).
    First,
    /// One prefetch refill, then the ALU (register-direct source).
    AfterPrefetch,
    /// Operand read, prefetch refill(s), then the ALU (memory source).
    Last,
}

/// A fixed-capacity staging buffer for one instruction's micro-op recipe. Holds up to [`MAX_OPS`] ops in
/// a plain array (no heap, no `Vec`) and hands the filled prefix to [`MicroState::from_ops`].
pub struct RecipeBuf {
    ops: [MicroOp; MAX_OPS],
    len: usize,
}

impl RecipeBuf {
    /// An empty buffer (filler slots are inert `Internal { cycles: 0 }`, identical to what
    /// [`MicroState::from_ops`] pads with, so a built recipe compares equal to the literal one).
    pub fn new() -> Self {
        Self {
            ops: [MicroOp::Internal { cycles: 0 }; MAX_OPS],
            len: 0,
        }
    }

    /// Append one micro-op. Panics if the recipe would exceed [`MAX_OPS`].
    pub fn push(&mut self, op: MicroOp) {
        assert!(self.len < MAX_OPS, "recipe exceeds MAX_OPS");
        self.ops[self.len] = op;
        self.len += 1;
    }

    /// The micro-ops pushed so far, in order.
    pub fn as_ops(&self) -> &[MicroOp] {
        &self.ops[..self.len]
    }

    /// Finalize into the resumable [`MicroState`] the drivers execute.
    pub fn finish(&self) -> MicroState {
        MicroState::from_ops(self.as_ops())
    }
}

impl Default for RecipeBuf {
    fn default() -> Self {
        Self::new()
    }
}

/// One source EA mode, decoded into the pieces the shared assembler interleaves: the optional operand
/// READ (its address), how many `Prefetch` refills the instruction emits (= its word count, invariant 1),
/// the operand the ALU combines, and where the ALU sits relative to the refills ([`AluPlacement`]). This
/// is the per-mode row of the plan's verified source-read table, made data so a single emitter places the
/// ALU rather than each mode re-implementing the interleave.
struct SrcSeq {
    /// Address of the operand READ, or `None` for register-direct / immediate modes (no operand read).
    read_addr: Option<Operand>,
    /// Number of `Prefetch` refills (= instruction word count).
    prefetch: u8,
    /// The operand the ALU combines (the source value).
    operand: Operand,
    /// Where the ALU sits relative to the refill(s).
    placement: AluPlacement,
}

/// Decode a source EA mode into its [`SrcSeq`]. Covers C1's modes — `Dn` (0), `(An)` (2), `#imm` (7/4).
/// Other modes land in later commits.
fn src_seq(mode: u16, reg: u8) -> SrcSeq {
    match (mode, reg) {
        // Dn — data-register direct: no operand read; one refill, then combine the register.
        (0, _) => SrcSeq {
            read_addr: None,
            prefetch: 1,
            operand: Operand::DataRegLow16(reg),
            placement: AluPlacement::AfterPrefetch,
        },
        // (An) — address-register indirect: read the operand (→ scratch 0), refill, then combine it.
        (2, _) => SrcSeq {
            read_addr: Some(Operand::AddrReg(reg)),
            prefetch: 1,
            operand: Operand::Scratch(0),
            placement: AluPlacement::Last,
        },
        // #imm — the immediate is the queued word; the ALU captures `prefetch[1]` BEFORE the two refills
        // shift it out (placement First), then both refills run (the 2-word instruction's fetch).
        (7, 4) => SrcSeq {
            read_addr: None,
            prefetch: 2,
            operand: Operand::ImmWord,
            placement: AluPlacement::First,
        },
        _ => todo!("ea_src mode {mode}/{reg} not yet covered"),
    }
}

/// Push the source-EA sub-sequence for an `<ea>,Dn`-shaped instruction: the bus steps that fetch the
/// source operand, interleaved with the prefetch refill(s) and the opcode's ALU. `make_alu` builds the
/// `MicroOp::Alu` given the operand the ALU combines (a register operand, or `Scratch(0)` for a memory
/// read) — the op/size/destination are the caller's, only the source operand and its placement are the
/// EA's concern. The [`AluPlacement`] from [`src_seq`] is the load-bearing pivot the emitter honors.
///
/// Covers C1's source modes — `Dn` (0), `(An)` (2), `#imm` (7/4). Other modes land in later commits.
pub fn ea_src(buf: &mut RecipeBuf, mode: u16, reg: u8, make_alu: impl FnOnce(Operand) -> MicroOp) {
    let seq = src_seq(mode, reg);
    let alu = make_alu(seq.operand);
    // The operand READ, if any, is the second-to-last bus event (invariant 3) — always before the refills.
    if let Some(addr) = seq.read_addr {
        buf.push(MicroOp::Read {
            addr,
            fc: super::microop::Fc::Data,
            size: super::microop::Size::Word,
            dst: 0,
        });
    }
    match seq.placement {
        // ALU first, then every refill (#imm captures prefetch[1] before the refills shift it out).
        AluPlacement::First => {
            buf.push(alu);
            for _ in 0..seq.prefetch {
                buf.push(MicroOp::Prefetch);
            }
        }
        // One refill, then the ALU (register-direct: a single fetch, no operand read).
        AluPlacement::AfterPrefetch => {
            debug_assert_eq!(seq.prefetch, 1, "AfterPrefetch is the 1-word direct form");
            buf.push(MicroOp::Prefetch);
            buf.push(alu);
        }
        // Operand read already pushed; the final refill, then the ALU combines the scratch value.
        AluPlacement::Last => {
            for _ in 0..seq.prefetch {
                buf.push(MicroOp::Prefetch);
            }
            buf.push(alu);
        }
    }
}

/// Push the destination-EA sub-sequence for a `Dn,<ea>` (memory-destination) read-modify-write: read the
/// old memory value, refill prefetch, combine via the ALU, write the result back. `make_alu` builds the
/// `MicroOp::Alu` given the memory operand (the minuend) and the scratch destination it writes; the write
/// then stores that scratch slot at the same address.
///
/// Covers C1's destination mode — `(An)` (2). Other alterable-memory modes land in later commits.
pub fn ea_dst(buf: &mut RecipeBuf, mode: u16, reg: u8, make_alu: impl FnOnce(Operand) -> MicroOp) {
    match (mode, reg) {
        // (An): read old value into scratch 0, refill, ALU (memory is the minuend) → scratch 1, write
        // scratch 1 back to (An). Collapses to today's exact `Dn,(An)` recipe — no EaCalc.
        (2, _) => {
            buf.push(MicroOp::Read {
                addr: Operand::AddrReg(reg),
                fc: super::microop::Fc::Data,
                size: super::microop::Size::Word,
                dst: 0,
            });
            buf.push(MicroOp::Prefetch);
            buf.push(make_alu(Operand::Scratch(0)));
            buf.push(MicroOp::Write {
                addr: Operand::AddrReg(reg),
                fc: super::microop::Fc::Data,
                size: super::microop::Size::Word,
                value: Operand::Scratch(1),
            });
        }
        _ => todo!("ea_dst mode {mode}/{reg} not yet covered"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::m68000::microop::{AluOp, Dest, Fc, MicroState, Size};

    /// The opcode's ALU as a literal, for the regression fixtures. `<ea>,Dn` form: `Dn` is the minuend
    /// (`a`), the source EA supplies `b`, the result lands back in `Dn`.
    fn ea_dn_alu(dn: u8, b: Operand) -> MicroOp {
        MicroOp::Alu {
            op: AluOp::Add,
            size: Size::Word,
            a: Operand::DataRegLow16(dn),
            b,
            dst: Dest::DataRegLow16(dn),
        }
    }

    /// The opcode's ALU as a literal for the `Dn,(An)` form: the memory operand (`a`) is the minuend,
    /// `Dn` is `b`, the result lands in scratch 1 (then written back to memory).
    fn dn_ea_alu(dn: u8, a: Operand) -> MicroOp {
        MicroOp::Alu {
            op: AluOp::Add,
            size: Size::Word,
            a,
            b: Operand::DataRegLow16(dn),
            dst: Dest::Scratch(1),
        }
    }

    fn build_src(mode: u16, reg: u8, dn: u8) -> MicroState {
        let mut buf = RecipeBuf::new();
        ea_src(&mut buf, mode, reg, |b| ea_dn_alu(dn, b));
        buf.finish()
    }

    fn build_dst(mode: u16, reg: u8, dn: u8) -> MicroState {
        let mut buf = RecipeBuf::new();
        ea_dst(&mut buf, mode, reg, |a| dn_ea_alu(dn, a));
        buf.finish()
    }

    // --- Regression guard: the builder emits byte-for-byte the SAME micro-op sequences as today's
    // literal recipes for the C1 modes (Dn, (An), #imm source; Dn,(An) dest). Any drift fails here. ---

    #[test]
    fn builder_matches_literal_dn_source() {
        // <op>.w Dn,Dn → [Prefetch, Alu(b=DataRegLow16(reg))].
        let literal =
            MicroState::from_ops(&[MicroOp::Prefetch, ea_dn_alu(3, Operand::DataRegLow16(5))]);
        assert_eq!(build_src(0, 5, 3), literal);
    }

    #[test]
    fn builder_matches_literal_an_indirect_source() {
        // <op>.w (An),Dn → [Read(AddrReg(reg)), Prefetch, Alu(b=Scratch(0))].
        let literal = MicroState::from_ops(&[
            MicroOp::Read {
                addr: Operand::AddrReg(2),
                fc: Fc::Data,
                size: Size::Word,
                dst: 0,
            },
            MicroOp::Prefetch,
            ea_dn_alu(4, Operand::Scratch(0)),
        ]);
        assert_eq!(build_src(2, 2, 4), literal);
    }

    #[test]
    fn builder_matches_literal_imm_source() {
        // <op>.w #imm,Dn → [Alu(b=ImmWord), Prefetch, Prefetch] (ALU First).
        let literal = MicroState::from_ops(&[
            ea_dn_alu(7, Operand::ImmWord),
            MicroOp::Prefetch,
            MicroOp::Prefetch,
        ]);
        assert_eq!(build_src(7, 4, 7), literal);
    }

    #[test]
    fn builder_matches_literal_dn_an_destination() {
        // <op>.w Dn,(An) → [Read(AddrReg), Prefetch, Alu(a=Scratch(0)→Scratch(1)), Write(AddrReg)].
        let literal = MicroState::from_ops(&[
            MicroOp::Read {
                addr: Operand::AddrReg(6),
                fc: Fc::Data,
                size: Size::Word,
                dst: 0,
            },
            MicroOp::Prefetch,
            dn_ea_alu(1, Operand::Scratch(0)),
            MicroOp::Write {
                addr: Operand::AddrReg(6),
                fc: Fc::Data,
                size: Size::Word,
                value: Operand::Scratch(1),
            },
        ]);
        assert_eq!(build_dst(2, 6, 1), literal);
    }
}
