//! Stack-based (ADaPT-style) decomposition: unlike [`crate::decompose`],
//! which writes a whole plan graph once, up front, this asks for exactly
//! one next action at a time - driven by what has actually happened so
//! far - and lets a step that turns out too big for one action be
//! decomposed further, as its own pushed sub-goal, only when that becomes
//! necessary. This is the escalation path the one-shot planner has no
//! equivalent of: a later step can see the *real* result of an earlier
//! one (not a placeholder it has to resolve itself), and a step nobody
//! could have sized correctly up front can still be broken down mid-run.
//!
//! The crate does not know how to execute a step - only the caller does,
//! whatever domain it's in - so [`run`] takes an `executor` closure rather
//! than owning any notion of what a step actually does.

use anyhow::{bail, Context, Result};

use crate::decompose::Completer;
use crate::next_action::{self, NextAction};

/// One step already taken within a frame, and what it produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub tag: String,
    pub text: String,
    pub result: String,
}

/// One goal being worked on: what it is, and everything done toward it so
/// far. A [`Stack`] holds one of these per level of decomposition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub goal: String,
    pub history: Vec<HistoryEntry>,
}

impl Frame {
    /// Renders the goal and its history so far as plain text, to splice
    /// into your own prompt template - this crate owns no prompt wording.
    pub fn render(&self) -> String {
        let mut out = format!("Goal: {}\n", self.goal);
        if self.history.is_empty() {
            out.push_str("(nothing done yet)\n");
        } else {
            for (i, h) in self.history.iter().enumerate() {
                let tag = if h.tag.is_empty() { "-" } else { &h.tag };
                out.push_str(&format!("{}. [{}] {} -> {}\n", i + 1, tag, h.text, h.result));
            }
        }
        out
    }
}

/// A stack of goals: the one currently being worked on is always the top.
/// Pushing happens when a step proves too complex to run directly;
/// popping happens when a pushed sub-goal resolves, feeding its answer
/// back as a completed step's result in the frame that pushed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stack {
    frames: Vec<Frame>,
}

impl Stack {
    pub fn new(goal: impl Into<String>) -> Self {
        Self { frames: vec![Frame { goal: goal.into(), history: Vec::new() }] }
    }

    /// The frame currently being worked on.
    pub fn current(&self) -> &Frame {
        self.frames.last().expect("a Stack always has at least the root frame")
    }

    /// True when there is nothing above the root goal.
    pub fn is_root(&self) -> bool {
        self.frames.len() == 1
    }

    /// How many levels deep the stack currently is (1 = just the root).
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Records a step's result against the current frame - call after
    /// executing a [`NextAction::Step`] yourself.
    pub fn record_step_result(
        &mut self,
        tag: impl Into<String>,
        text: impl Into<String>,
        result: impl Into<String>,
    ) {
        self.frames.last_mut().unwrap().history.push(HistoryEntry {
            tag: tag.into(),
            text: text.into(),
            result: result.into(),
        });
    }

    /// Pushes a new sub-goal on top - call after a [`NextAction::Decompose`].
    pub fn push_subgoal(&mut self, subgoal: impl Into<String>) {
        self.frames.push(Frame { goal: subgoal.into(), history: Vec::new() });
    }

    /// Pops the current (non-root) frame, feeding its resolution back into
    /// the parent as a completed step. `tag`/`text` describe, from the
    /// parent's point of view, what that pushed sub-goal was; `result` is
    /// its resolved answer. Fails on the root frame - a resolved root is
    /// the whole decomposition's answer, not a step result, and the
    /// caller should stop the loop instead of popping.
    pub fn pop_with_result(
        &mut self,
        tag: impl Into<String>,
        text: impl Into<String>,
        result: impl Into<String>,
    ) -> Result<()> {
        if self.is_root() {
            bail!("cannot pop the root frame - it has no parent to report back to");
        }
        self.frames.pop();
        self.record_step_result(tag, text, result);
        Ok(())
    }
}

/// What [`run`] ended with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The root goal resolved, with this answer.
    Done(String),
    /// A step asked for the human before the root goal resolved.
    NeedsHuman(String),
}

/// Drives the stack-based loop to completion. At each turn: builds a
/// prompt from the current frame via `prompt_builder`, asks `completer`,
/// parses the one next action, and either runs it via `executor`, pushes
/// a further sub-goal, or resolves - popping back up the stack until the
/// root goal itself resolves.
///
/// `max_actions` bounds the whole run (across every frame) against a model
/// that never says `DONE`.
pub fn run(
    goal: impl Into<String>,
    completer: &dyn Completer,
    max_actions: usize,
    mut prompt_builder: impl FnMut(&Frame) -> String,
    mut executor: impl FnMut(&str, &str) -> Result<String>,
) -> Result<Outcome> {
    let mut stack = Stack::new(goal);
    for _ in 0..max_actions {
        let prompt = prompt_builder(stack.current());
        let response = completer.complete(&prompt)?;
        let action = next_action::parse(&response)
            .with_context(|| format!("model response named no recognized next action: {response:?}"))?;

        match action {
            NextAction::Step { tag, text } => {
                let result = executor(&tag, &text)?;
                stack.record_step_result(tag, text, result);
            }
            NextAction::Decompose { subgoal } => {
                stack.push_subgoal(subgoal);
            }
            NextAction::Done { answer } => {
                if stack.is_root() {
                    return Ok(Outcome::Done(answer));
                }
                let resolved_goal = stack.current().goal.clone();
                stack.pop_with_result("resolved", resolved_goal, answer)?;
            }
            NextAction::Ask { question } => return Ok(Outcome::NeedsHuman(question)),
        }
    }
    bail!("exceeded max_actions ({max_actions}) without resolving the goal")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct ScriptedCompleter {
        responses: RefCell<std::collections::VecDeque<&'static str>>,
    }

    impl ScriptedCompleter {
        fn new(responses: &[&'static str]) -> Self {
            Self { responses: RefCell::new(responses.iter().copied().collect()) }
        }
    }

    impl Completer for ScriptedCompleter {
        fn complete(&self, _prompt: &str) -> Result<String> {
            self.responses
                .borrow_mut()
                .pop_front()
                .map(|s| s.to_string())
                .context("scripted completer ran out of responses")
        }
    }

    #[test]
    fn stack_push_pop_round_trips_through_the_parent() {
        let mut stack = Stack::new("root goal");
        assert!(stack.is_root());
        stack.push_subgoal("sub goal");
        assert!(!stack.is_root());
        assert_eq!(stack.current().goal, "sub goal");
        stack.pop_with_result("resolved", "sub goal", "sub answer").unwrap();
        assert!(stack.is_root());
        assert_eq!(stack.current().history.len(), 1);
        assert_eq!(stack.current().history[0].result, "sub answer");
    }

    #[test]
    fn popping_the_root_fails() {
        let mut stack = Stack::new("root goal");
        assert!(stack.pop_with_result("x", "y", "z").is_err());
    }

    #[test]
    fn run_resolves_a_simple_chain_using_real_intermediate_results() {
        // Each next step is written by the (scripted) model AFTER seeing
        // the real result of the step before it - "history of XYZ", not a
        // placeholder like one-shot decompose would need substituted in.
        let completer = ScriptedCompleter::new(&[
            "STEP: [retrieve] the capital of ABC",
            "STEP: [retrieve] the history of XYZ",
            "DONE: XYZ is the capital; its history spans three centuries.",
        ]);

        let outcome = run(
            "what is the capital of ABC, and what is its history?",
            &completer,
            10,
            |frame| frame.render(),
            |tag, text| {
                if text.contains("capital") {
                    assert_eq!(tag, "retrieve");
                    Ok("XYZ".to_string())
                } else {
                    Ok("three centuries of recorded history".to_string())
                }
            },
        )
        .unwrap();

        assert_eq!(
            outcome,
            Outcome::Done("XYZ is the capital; its history spans three centuries.".to_string())
        );
    }

    #[test]
    fn run_decomposes_a_step_and_pops_back_to_the_parent() {
        let completer = ScriptedCompleter::new(&[
            "DECOMPOSE: design the trie's API first",
            "STEP: [design] pick insert/search/starts_with as the API",
            "DONE: fn insert(&mut self, word: &str); fn search(&self, word: &str) -> bool;",
            "DONE: implemented a trie with the designed API and tests.",
        ]);

        let mut depths_seen = Vec::new();
        let outcome = run(
            "build a small trie library with tests",
            &completer,
            10,
            |frame| {
                depths_seen.push(frame.goal.clone());
                frame.render()
            },
            |_tag, _text| Ok("done".to_string()),
        )
        .unwrap();

        assert_eq!(
            outcome,
            Outcome::Done("implemented a trie with the designed API and tests.".to_string())
        );
        // The pushed sub-goal's own prompt saw ITS goal, not the root's.
        assert!(depths_seen.contains(&"design the trie's API first".to_string()));
    }

    #[test]
    fn run_stops_early_when_a_step_asks_for_the_human() {
        let completer = ScriptedCompleter::new(&["ASK: which ABC do you mean?"]);
        let outcome = run(
            "what is the capital of ABC?",
            &completer,
            10,
            |frame| frame.render(),
            |_tag, _text| Ok(String::new()),
        )
        .unwrap();
        assert_eq!(outcome, Outcome::NeedsHuman("which ABC do you mean?".to_string()));
    }

    #[test]
    fn run_gives_up_after_max_actions() {
        // A completer that always returns a Step and never Done, capped low.
        struct Loops;
        impl Completer for Loops {
            fn complete(&self, _prompt: &str) -> Result<String> {
                Ok("STEP: [retrieve] a".to_string())
            }
        }
        let result = run("never resolves", &Loops, 3, |frame| frame.render(), |_, _| Ok("ok".to_string()));
        assert!(result.is_err());
    }
}
