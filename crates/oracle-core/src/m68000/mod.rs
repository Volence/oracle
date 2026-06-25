//! Motorola 68000 core (Phase-0 vertical slice).
//!
//! Bootstrapped in spirit from the MIT Exodus core (`../oracle/Devices/M68000`), re-architected for
//! tick-stepping. Phase 0 implements just enough — the register file plus the `ADD.w Dn,(An)` family —
//! to (a) gate on real SingleStepTests data and (b) settle the cycle-granularity decision by running
//! the same opcode both instruction-stepped and FSM-quiesced (see [`prototype`]).

pub mod bus68k;
pub mod microop;
pub mod prototype;
pub mod registers;

pub use registers::Registers;
