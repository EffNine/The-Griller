//! `griller-core` contains the pure, deterministic foundation of The Griller.
//!
//! Everything in this crate is data: value types, serialization, and hashing.
//! It must never perform filesystem I/O, process execution, network access, or
//! depend on any environment-specific runtime behaviour. Nothing here calls an
//! LLM and nothing here reads the world; collectors and execution live in
//! `griller-runtime`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod attack;
pub mod command;
pub mod context;
pub mod error;
pub mod evidence;
pub mod hash;
pub mod provenance;
pub mod redaction;
pub mod snapshot;
