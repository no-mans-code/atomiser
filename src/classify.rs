//! Deterministic classification: a caller-registered chain of matchers, no
//! model call. The type vocabulary and matchers are entirely caller-supplied
//! - this crate owns no domain knowledge of its own.

/// One classifier: a type tag and the matcher that claims it.
struct Rule {
    task_type: String,
    matches: Box<dyn Fn(&str) -> bool + Send + Sync>,
}

/// An ordered chain of classifiers. First match wins, so registration order
/// is match priority.
#[derive(Default)]
pub struct Registry {
    rules: Vec<Rule>,
}

impl Registry {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Adds one classifier to the chain.
    pub fn register(
        &mut self,
        task_type: impl Into<String>,
        matches: impl Fn(&str) -> bool + Send + Sync + 'static,
    ) {
        self.rules.push(Rule {
            task_type: task_type.into(),
            matches: Box::new(matches),
        });
    }

    /// Returns the first matching task type, or "general" if nothing in the
    /// chain claims this text.
    pub fn classify(&self, text: &str) -> String {
        for rule in &self.rules {
            if (rule.matches)(text) {
                return rule.task_type.clone();
            }
        }
        "general".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_match_wins() {
        let mut reg = Registry::new();
        reg.register("narrow", |t: &str| t.contains("foo"));
        reg.register("broad", |t: &str| t.contains("o"));
        assert_eq!(reg.classify("foo"), "narrow");
        assert_eq!(reg.classify("bob"), "broad");
    }

    #[test]
    fn falls_back_to_general() {
        let mut reg = Registry::new();
        reg.register("narrow", |t: &str| t.contains("foo"));
        assert_eq!(reg.classify("nothing here"), "general");
    }
}
