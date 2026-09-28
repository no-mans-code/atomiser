//! Detects whether a request names more than one distinct thing to do - the
//! signal that it needs decomposition rather than one direct step. Keeps no
//! domain-specific carve-outs (e.g. file-write-specific cases): those belong
//! in a caller's own domain layer, not in a reusable decomposition primitive.

use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

/// Explicit sequencing/joining language. A conditional fallback ("if you
/// don't know X, search for it") counts too - it joins two real actions as
/// surely as "and" does.
static JOINER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(and then|then|after that|and also|, then|and save|and write|and store|and put|and run|and download|and summari[sz]e|if you (don't|do not|aren't|are not) (know|sure|certain))\b",
    )
    .unwrap()
});

/// Verbs that name a distinct action. Two or more distinct ones, joined,
/// means a request needs a real plan rather than one step.
static ACTION_VERB_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(search|google|look up|research|browse|find out|check|download|fetch|write|create|save|generate|run|execute|install|build|test|summari[sz]e|compare|analyse|analyze)\b",
    )
    .unwrap()
});

/// Reports whether a request asks for more than one distinct thing: a
/// joiner is present AND it names two or more distinct action verbs. A
/// single action repeated ("write and save the file") is not compound on
/// its own - which is why the joiner alone is not enough.
pub fn looks_compound(text: &str) -> bool {
    if !JOINER_RE.is_match(text) {
        return false;
    }
    let mut seen: HashSet<String> = HashSet::new();
    for m in ACTION_VERB_RE.find_iter(text) {
        seen.insert(m.as_str().to_lowercase());
    }
    seen.len() >= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_verbs_with_joiner_is_compound() {
        assert!(looks_compound(
            "search the web for the newest Go version and then write a note about it"
        ));
    }

    #[test]
    fn single_action_is_not_compound() {
        assert!(!looks_compound("write a short note about the newest Go version"));
    }

    #[test]
    fn no_joiner_is_not_compound() {
        assert!(!looks_compound("search the web, download the CSV, run the script"));
    }
}
