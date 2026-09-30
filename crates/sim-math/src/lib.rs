//! Deterministic math for the SimAgent simulation core.
//!
//! Every nonlinear function the simulation needs (exp, ln, pow, sqrt, floor, ...) is
//! provided here on top of a pinned `libm`, so that results are bit identical on every
//! supported target (ADR-0003). The float methods of `std` are banned everywhere else.
//! The functions arrive with the code that first needs them (HANDOFF §9 step 2).

#![no_std]
