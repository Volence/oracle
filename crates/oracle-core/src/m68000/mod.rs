//! Motorola 68000 core.
//!
//! Bootstrapped in spirit from the MIT Exodus core (`../oracle-old/Devices/M68000`), re-architected around a
//! single micro-op framework: every instruction (and every exception entry) is a [`microop::MicroState`]
//! recipe of [`microop::MicroOp`]s executed by one `exec_one`, reached by both drivers — the fast
//! `run_instruction`/`step` path and the on-demand `step_micro_op` quiesce path — so behavior is defined
//! exactly once. Decode is total over all 65536 opcodes; the exception/async cluster (illegal, privilege,
//! trace, autovectored interrupts, STOP) is complete. The core is gated against real SingleStepTests data
//! over the [`bus68k::Bus68k`] trait (the `FlatBus` harness); a machine-side adapter (`MegaDriveBus`, on
//! the `crate::bus` side) implements the same trait over the real memory map.

pub mod bus68k;
pub mod decode;
pub mod ea;
pub mod exception;
pub mod microop;
pub mod registers;

pub use registers::Registers;
