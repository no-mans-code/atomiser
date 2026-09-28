//! One-shot decomposition: ask a model to write a numbered step graph for a
//! request that [`crate::compound::looks_compound`] says needs one, then
//! parse it with [`crate::plan::parse`]. This is the "plan-and-execute"
//! half of the atomiser - the cheap, deterministic-friendly default.
//!
//! The heavier, iterative escalation (ADaPT-style: re-decompose a step only
//! when *running* it proves the one-shot plan insufficient) is intentionally
//! not implemented here - it is a separate, larger piece of work tracked as
//! a follow-up, not a partial stub bolted onto this one.

use anyhow::Result;

use crate::plan::{self, Plan};

/// Whatever can turn a prompt into a model completion. The caller owns the
/// model, the endpoint, and the temperature (0, for reproducibility, is the
/// caller's responsibility) - this crate has no opinion on how you reach
/// your model, only on the shape of the response it parses.
pub trait Completer {
    fn complete(&self, prompt: &str) -> Result<String>;
}

/// Runs one decomposition call and parses its response into a [`Plan`].
/// `prompt` is entirely the caller's own - this crate carries no fixed tool
/// vocabulary or planning rules, only the numbered/tagged/dependency format
/// the response is parsed as.
pub fn decompose(completer: &dyn Completer, prompt: &str) -> Result<Plan> {
    let response = completer.complete(prompt)?;
    let steps = plan::parse(&response);
    Ok(Plan { steps })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedCompleter(&'static str);
    impl Completer for FixedCompleter {
        fn complete(&self, _prompt: &str) -> Result<String> {
            Ok(self.0.to_string())
        }
    }

    #[test]
    fn decompose_parses_the_completion_into_a_plan() {
        let completer = FixedCompleter(
            "1. [retrieve] the capital of ABC\n\
             2. [retrieve] the history of the capital found in step 1 (after 1)\n\
             3. [answer] synthesize the answer using steps 1 and 2 (after 1, 2)",
        );
        let plan = decompose(&completer, "irrelevant for this fake completer").unwrap();
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.steps[2].needs, vec![1, 2]);
    }
}
