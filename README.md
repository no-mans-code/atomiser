# atomiser

Reusable query/task decomposition for LLM-backed tools: a deterministic classify + compound-detection layer (no model call), and two ways to actually decompose a request once that layer says it needs it — **one-shot** (write the whole step graph up front, one call) and **stack-based** (ask for one action at a time, seeing real results as they come in, escalating to further decomposition only when needed).

No domain vocabulary is built in — register your own classifiers, supply your own prompts, and this crate stays out of your way.

## Design

A three-layer task router for an agent (`Classify` → `Triage` → `Planner`) built around one hard-won rule: never ask a model to judge something a deterministic check can decide instead. No domain vocabulary is baked in — no fixed tool set, no hardcoded routing rules — this crate keeps only the reusable mechanism: the classify registry, the compound-request detector, and the numbered/tagged/dependency step-graph parser — so any project can register its own vocabulary on top of it. Its use for RAG step types in a document-intelligence engine lives in [docuzent](https://github.com/no-mans-code/docuzent)'s `docuzent-core`.

## Usage

Add it as a git dependency (not yet published to crates.io):

```toml
[dependencies]
atomiser = { git = "https://github.com/no-mans-code/atomiser" }
```

### 1. Classify (no model call)

Register your own types and matchers — the crate has no opinion on your vocabulary:

```rust
use atomiser::classify::Registry;

let mut registry = Registry::new();
registry.register("fact-lookup", |t: &str| t.starts_with("what is") || t.starts_with("who is"));
registry.register("comparison", |t: &str| t.contains(" vs ") || t.contains("compare"));

assert_eq!(registry.classify("what is the capital of France?"), "fact-lookup");
assert_eq!(registry.classify("good morning"), "general"); // nothing claimed it
```

### 2. Detect whether a request needs decomposing at all (no model call)

```rust
use atomiser::compound::looks_compound;

assert!(looks_compound(
    "search the web for the newest Go version and then write a note about it"
));
assert!(!looks_compound("what is the capital of France?"));
```

### 3. Decompose (one model call, only when `looks_compound` says so)

Implement `Completer` over whatever model client you already have — this crate has no opinion on how you reach your model, only on the shape of the response it parses:

```rust
use atomiser::{decompose, Completer, Plan};
use anyhow::Result;

struct MyOllamaClient { /* ... */ }

impl Completer for MyOllamaClient {
    fn complete(&self, prompt: &str) -> Result<String> {
        // call your model at temperature 0 here, return its raw text
        todo!()
    }
}

let prompt = format!(
    "Plan the request below as numbered steps. Tag each with [retrieve] or [answer]. \
     When a step uses an earlier step's result, end it with (after N).\n\nRequest: {}",
    "what is the capital of ABC, and what is its history?"
);

let plan: Plan = decompose(&my_completer, &prompt)?;
for step in &plan.steps {
    println!("{}. [{}] {} (after {:?})", step.id, step.tag, step.text, step.needs);
}
// 1. [retrieve] the capital of ABC (after [])
// 2. [retrieve] the history of the capital found in step 1 (after [1])
// 3. [answer] synthesize the answer using steps 1 and 2 (after [1, 2])
```

The parser (`atomiser::plan::parse`) is the reusable part of this step — it turns any numbered, `[tag]`-annotated, `(after N, M)`-annotated model response into a `Plan` of `Step`s with resolved, dense, cycle-safe dependency ids. An untagged or unannotated step defaults to depending on the step right before it, so a model that forgets to annotate still produces a runnable (sequential) plan.

### 4. Or: stack-based decomposition (ADaPT-style)

One-shot writes "the history of the capital found in step 1" and leaves the executor to substitute in the real value. Stack-based instead asks the model for one action at a time, and by the second call it already knows the capital is "XYZ" — no substitution needed, because the model saw the real result before writing the next step. It can also decompose a step further, mid-run, if that step turns out too big for one action — something a plan written entirely up front cannot do.

```rust
use atomiser::{run_iterative, Completer, Outcome};
use anyhow::Result;

# struct MyOllamaClient;
# impl Completer for MyOllamaClient {
#     fn complete(&self, _prompt: &str) -> Result<String> { unimplemented!() }
# }
# fn doc_example(my_completer: MyOllamaClient) -> Result<()> {
let instructions = "\
At each turn, reply with exactly one line: \
STEP: [tag] text  |  DECOMPOSE: subgoal  |  DONE: final answer  |  ASK: question";

let outcome = run_iterative(
    "what is the capital of ABC, and what is its history?",
    &my_completer,
    10, // max_actions - a safety cap against a model that never says DONE
    |frame| format!("{instructions}\n\n{}", frame.render()),
    |tag, text| {
        // execute the named step however your domain does that, return its result
        Ok(format!("resolved: {tag} {text}"))
    },
)?;

match outcome {
    Outcome::Done(answer) => println!("{answer}"),
    Outcome::NeedsHuman(question) => println!("need to ask: {question}"),
}
# Ok(())
# }
```

`run_iterative` is the ergonomic entry point; [`Stack`], [`Frame`], and `next_action::parse` are exposed separately if you want to drive the loop by hand (e.g. to persist state between turns, or run several stacks concurrently).

## Not yet implemented

Nothing tracked right now - both decomposition strategies described above are implemented and tested. The natural next question (which strategy to use when, and whether one-shot's output could seed a stack-based escalation instead of running standalone) is open design space, not a gap in either implementation.

See `src/lib.rs` for the fuller design note.

## License

MIT
