//! The single-action protocol a stack-based decomposition turn's model
//! response is parsed as: exactly one of `STEP`, `DECOMPOSE`, `DONE`, or
//! `ASK`, on its own line (surrounding prose or markdown is fine - the
//! first recognized line wins). Unlike [`crate::plan`], which parses a
//! whole numbered graph written once, this parses one action at a time,
//! since a stack-based turn only ever asks "what's next" given what has
//! actually happened so far.

use std::sync::LazyLock;

use regex::Regex;

/// One turn's next action, as the model named it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextAction {
    /// Run this one step next; the caller executes it however it knows
    /// how to, then reports the result back via
    /// [`crate::iterative::Stack::record_step_result`].
    Step { tag: String, text: String },
    /// This step is too complex to run directly - decompose it further as
    /// its own sub-goal, pushed via
    /// [`crate::iterative::Stack::push_subgoal`].
    Decompose { subgoal: String },
    /// The current goal is resolved.
    Done { answer: String },
    /// Cannot proceed without the human.
    Ask { question: String },
}

static STEP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^STEP\s*:\s*(?:\[\s*([A-Za-z-]+)\s*\]\s*)?(.+)$").unwrap());
static DECOMPOSE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^DECOMPOSE\s*:\s*(.+)$").unwrap());
static DONE_HEAD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^DONE\s*:\s*(.*)$").unwrap());
static ASK_HEAD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^ASK\s*:\s*(.*)$").unwrap());

/// Parses a model's response into the one next action it named. `DONE` and
/// `ASK` take the rest of the response (their answer/question may run
/// several lines); `STEP` and `DECOMPOSE` are single-line, matching a
/// single next action's scope.
pub fn parse(response: &str) -> Option<NextAction> {
    let lines: Vec<&str> = response.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if let Some(m) = STEP_RE.captures(trimmed) {
            let tag = m.get(1).map(|t| t.as_str().to_lowercase()).unwrap_or_default();
            let text = m[2].trim().to_string();
            if !text.is_empty() {
                return Some(NextAction::Step { tag, text });
            }
        }
        if let Some(m) = DECOMPOSE_RE.captures(trimmed) {
            let subgoal = m[1].trim().to_string();
            if !subgoal.is_empty() {
                return Some(NextAction::Decompose { subgoal });
            }
        }
        if let Some(m) = DONE_HEAD_RE.captures(trimmed) {
            if let Some(answer) = rest_of_response(&m[1], &lines[i + 1..]) {
                return Some(NextAction::Done { answer });
            }
        }
        if let Some(m) = ASK_HEAD_RE.captures(trimmed) {
            if let Some(question) = rest_of_response(&m[1], &lines[i + 1..]) {
                return Some(NextAction::Ask { question });
            }
        }
    }
    None
}

fn rest_of_response(head: &str, tail_lines: &[&str]) -> Option<String> {
    let mut out = head.trim().to_string();
    for line in tail_lines {
        out.push('\n');
        out.push_str(line);
    }
    let out = out.trim().to_string();
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_tagged_step() {
        assert_eq!(
            parse("STEP: [retrieve] the capital of ABC"),
            Some(NextAction::Step { tag: "retrieve".into(), text: "the capital of ABC".into() })
        );
    }

    #[test]
    fn parses_an_untagged_step() {
        assert_eq!(
            parse("STEP: just do this"),
            Some(NextAction::Step { tag: String::new(), text: "just do this".into() })
        );
    }

    #[test]
    fn parses_decompose() {
        assert_eq!(
            parse("DECOMPOSE: design the trie's API first"),
            Some(NextAction::Decompose { subgoal: "design the trie's API first".into() })
        );
    }

    #[test]
    fn parses_done_with_multiline_answer() {
        let response = "DONE: XYZ is the capital.\nIts history spans three centuries.";
        assert_eq!(
            parse(response),
            Some(NextAction::Done { answer: "XYZ is the capital.\nIts history spans three centuries.".into() })
        );
    }

    #[test]
    fn parses_ask() {
        assert_eq!(
            parse("ASK: which ABC do you mean - the country or the company?"),
            Some(NextAction::Ask { question: "which ABC do you mean - the country or the company?".into() })
        );
    }

    #[test]
    fn ignores_surrounding_prose() {
        let response = "Sure, here's what I'll do next.\nSTEP: [retrieve] the capital of ABC\nHope that helps!";
        assert_eq!(
            parse(response),
            Some(NextAction::Step { tag: "retrieve".into(), text: "the capital of ABC".into() })
        );
    }

    #[test]
    fn returns_none_when_nothing_recognized() {
        assert_eq!(parse("I'm not sure what to do."), None);
    }
}
