//! atomiser: reusable query/task decomposition.
//!
//! Two decomposition strategies, both built on the same cheap, no-model
//! foundation:
//!
//! 1. [`classify`] - a caller-registered chain of deterministic matchers
//!    that tags a request with a type. No domain vocabulary is built in;
//!    register your own types and matchers.
//! 2. [`compound`] - deterministically detects whether a request names more
//!    than one distinct thing to do, the signal that it needs decomposition
//!    at all.
//!
//! From there, two ways to actually decompose a request `compound` says
//! needs it:
//!
//! - [`decompose`] + [`plan`] - **one-shot**: one model call writes a
//!   numbered, tagged step graph up front, parsed into a [`plan::Plan`] of
//!   [`plan::Step`]s with `needs` dependencies on earlier steps. Cheap,
//!   deterministic-friendly (one call, easy to cache), but a later step's
//!   text is written blind to earlier steps' *actual* results - the
//!   executor has to substitute them in.
//! - [`iterative`] + [`next_action`] - **stack-based** (ADaPT-style): one
//!   model call per *action*, each seeing the real results of everything
//!   done so far, and able to decompose a step further - pushing it as its
//!   own sub-goal - only when running it proves that necessary. More
//!   model calls, but handles chains and branching a one-shot plan can't
//!   size correctly up front.
//!
//! Ported from a proven Go design (a sibling project's own
//! `internal/atomizer`), generalized: that project baked in one agent's
//! tool vocabulary and routing rules; this crate keeps only the reusable
//! mechanism so any project can register its own.

pub mod classify;
pub mod compound;
pub mod decompose;
pub mod iterative;
pub mod next_action;
pub mod plan;

pub use classify::Registry;
pub use compound::looks_compound;
pub use decompose::{decompose, Completer};
pub use iterative::{run as run_iterative, Frame, Outcome, Stack};
pub use next_action::NextAction;
pub use plan::{Plan, Step};
