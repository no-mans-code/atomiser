//! The step graph atomiser produces: a numbered, tagged list a model wrote,
//! parsed into steps with dependencies. There is no hardcoded tool
//! vocabulary here - a step's tag is whatever the model wrote in brackets
//! (or empty when untagged), which the caller is free to post-process
//! against its own `classify::Registry`.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

/// One node of a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// 1-based position in the plan. `needs` refers to these ids.
    pub id: usize,
    /// The step's text, tag and dependency annotation stripped.
    pub text: String,
    /// The tag the model wrote ("web", "answer", ...), lowercased; empty
    /// if the line carried none.
    pub tag: String,
    /// Ids of earlier steps this one depends on, ascending, deduplicated.
    pub needs: Vec<usize>,
}

/// A parsed plan: the steps to run, in dependency order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub steps: Vec<Step>,
}

impl Plan {
    pub fn texts(&self) -> Vec<&str> {
        self.steps.iter().map(|s| s.text.as_str()).collect()
    }
}

static STEP_LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?im)^\s*(?:step\s*)?(\d+)\s*[.):-]\s*(.+)$").unwrap());
static TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:\*\*)?\[\s*([A-Za-z-]+)\s*\](?:\*\*)?\s*:?\s*(.*)$").unwrap());
static NEEDS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s*\(\s*(?:after|needs?|uses?|using|depends\s+on|from|based\s+on)\s*:?\s*(?:steps?\s*)?([0-9][0-9,\s&and]*)\)\s*\.?\s*$").unwrap()
});
static INDEPENDENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s*\(\s*(?:independent|parallel|after\s+none|needs\s+nothing|no\s+dependencies)\s*\)\s*\.?\s*$").unwrap()
});
static NEEDS_NUM_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());

struct RawLine {
    num: usize,
    text: String,
    tag: String,
    needs: Vec<usize>,
    given: bool,
}

/// Parses a model's numbered, optionally tagged list into steps: dependency
/// ids remapped to dense ids, and any step the model did not annotate
/// itself defaults to a sequential dependency on the step right before it.
pub fn parse(text: &str) -> Vec<Step> {
    let mut parsed = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        let Some(m) = STEP_LINE_RE.captures(line) else {
            continue;
        };
        let num: usize = match m[1].parse() {
            Ok(n) => n,
            Err(_) => continue,
        };
        let mut body = m[2].trim().to_string();

        let mut needs = Vec::new();
        let mut given = false;
        if let Some(nm) = NEEDS_RE.captures(&body) {
            for d in NEEDS_NUM_RE.find_iter(&nm[1]) {
                if let Ok(n) = d.as_str().parse::<usize>() {
                    if n < num {
                        needs.push(n);
                    }
                }
            }
            given = true;
            let cut = nm.get(0).unwrap().start();
            body.truncate(cut);
            body = body.trim().to_string();
        } else if let Some(im) = INDEPENDENT_RE.find(&body) {
            given = true;
            let cut = im.start();
            body.truncate(cut);
            body = body.trim().to_string();
        }

        let mut tag = String::new();
        if let Some(tm) = TAG_RE.captures(&body) {
            tag = tm[1].to_lowercase();
            body = tm[2].trim().to_string();
        }
        body = body.trim_matches('*').trim().to_string();
        if body.is_empty() {
            continue;
        }

        parsed.push(RawLine { num, text: body, tag, needs, given });
    }

    let mut id_for = HashMap::new();
    for (i, r) in parsed.iter().enumerate() {
        id_for.insert(r.num, i + 1);
    }

    let mut steps: Vec<Step> = Vec::with_capacity(parsed.len());
    let mut given_flags: Vec<bool> = Vec::with_capacity(parsed.len());
    for (i, r) in parsed.iter().enumerate() {
        let id = i + 1;
        let mut needs = Vec::new();
        if r.given {
            let mut seen = HashSet::new();
            for &n in &r.needs {
                if let Some(&mapped) = id_for.get(&n) {
                    if mapped < id && seen.insert(mapped) {
                        needs.push(mapped);
                    }
                }
            }
            needs.sort_unstable();
        }
        given_flags.push(r.given);
        steps.push(Step {
            id,
            text: r.text.clone(),
            tag: r.tag.clone(),
            needs,
        });
    }

    wire_default_needs(&mut steps, &given_flags);
    steps
}

/// Fills in a strictly sequential dependency for any step the model did not
/// annotate itself - a plan is written in the order it should happen, so an
/// untagged step depends on the one right before it.
fn wire_default_needs(steps: &mut [Step], given: &[bool]) {
    for i in 0..steps.len() {
        if given[i] || i == 0 {
            continue;
        }
        let prev_id = steps[i - 1].id;
        steps[i].needs = vec![prev_id];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capital_then_history_chain() {
        let text = "\
1. [retrieve] the capital of ABC
2. [retrieve] the history of the capital found in step 1 (after 1)
3. [answer] synthesize the answer using steps 1 and 2 (after 1, 2)";

        let steps = parse(text);
        assert_eq!(steps.len(), 3);

        assert_eq!(steps[0].id, 1);
        assert_eq!(steps[0].tag, "retrieve");
        assert_eq!(steps[0].text, "the capital of ABC");
        assert!(steps[0].needs.is_empty());

        assert_eq!(steps[1].tag, "retrieve");
        assert_eq!(steps[1].text, "the history of the capital found in step 1");
        assert_eq!(steps[1].needs, vec![1]);

        assert_eq!(steps[2].tag, "answer");
        assert_eq!(steps[2].needs, vec![1, 2]);
    }

    #[test]
    fn untagged_steps_default_to_sequential() {
        let text = "1. do X\n2. do Y\n3. do Z";
        let steps = parse(text);
        assert_eq!(steps.len(), 3);
        assert!(steps[0].needs.is_empty());
        assert_eq!(steps[1].needs, vec![1]);
        assert_eq!(steps[2].needs, vec![2]);
    }

    #[test]
    fn independent_annotation_has_no_needs() {
        let text = "1. [web] search for X\n2. [web] search for Y (independent)";
        let steps = parse(text);
        assert!(steps[1].needs.is_empty());
    }

    #[test]
    fn dependency_on_dropped_or_future_step_is_ignored() {
        // Step numbers can be non-contiguous or point forward; only valid,
        // earlier references survive renumbering.
        let text = "5. [web] search for X\n9. [answer] use it (after 5, 12)";
        let steps = parse(text);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[1].needs, vec![1]);
    }
}
