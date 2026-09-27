//! `griller-runtime` implements everything that touches the world: process
//! execution, git interaction, repository discovery, evidence collectors, and
//! a JSON evidence store.
//!
//! Everything here is testable **without an LLM**. The dependency direction is
//! one-way: `griller-runtime` depends on `griller-core`, never the reverse.

// `deny` rather than `forbid`, and only in this crate.
//
// `forbid` cannot be overridden by a local `#[allow]`, and process-group teardown
// on timeout requires exactly one `unsafe` libc call (`killpg`). The alternative
// was leaving `cargo test` grandchildren alive after every timeout, which is a
// reproducibility defect in a system whose evidence model is content-addressed.
//
// The exception is deliberately confined: exactly one function in
// `execution::terminate_process_group` carries `#[allow(unsafe_code)]` and a
// SAFETY comment. Every other crate in the workspace keeps absolute
// `#![forbid(unsafe_code)]`, so `deny` still turns any *new* unsafe code anywhere
// in this crate into a compile error. Relaxed in Phase 2; see
// `PHASE_2_BASELINE.md` §6.
#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod attack;
pub mod collectors;
pub mod error;
pub mod execution;
pub mod git;
pub mod identity;
pub mod manifest;
pub mod store;
pub mod vocabulary;
pub mod worktree;
