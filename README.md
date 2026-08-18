# chart

A declarative controller framework for Rust. You describe what a controller
reacts to and what it does about it as static data — a state table, or a
simple list of inputs and outputs — and that single declaration drives the
runtime, generates a mermaid diagram, and gets checked for gaps.

[한국어 문서](README_KR.md)

## Why

Controller logic tends to spread across `match` arms until "what happens when
X arrives" can only be answered by reading the whole file, and the diagram
someone drew for it slowly stops matching reality. chart flips that: the
transition table (or feature list) *is* the source of truth, so the diagram
and the exhaustive coverage check are generated from the exact data the
executor runs. There's nothing to keep in sync because there's only one copy.

## Two layers, one vocabulary

| Layer | Use when | You declare |
|---|---|---|
| `feature` | behavior doesn't depend on history | each feature's events in, actions out |
| `machine` | the same event means different things in different states | a transition table |

Both layers share a `Domain` — the events, actions, and outside-world type a
controller works with — so a stateless feature that later needs history
doesn't change; you just add a small `MachineSpec` next to it. Most
controllers are mostly `feature`, with a `machine` where it's actually needed.

## Install

Not published to crates.io — use it as a path dependency.

```toml
[dependencies]
chart = { path = "../chart" }
```

## Quick start

A two-state light switch:

```rust
use chart::machine::{Edge, Goto, Ignore, Machine, OnUnknown, Source, State};
use chart::{Domain, MachineSpec};

chart::tags! { enum Tag { Off, On } }
chart::events! {
    #[derive(Clone, Debug)]
    enum Event => Kind { Toggle }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Action { TurnOn, TurnOff }

struct Env;
struct Light;

impl Domain for Light {
    type Event = Event;
    type EventKind = Kind;
    type Action = Action;
    type Env = Env;

    fn perform(action: Action, _ev: &Event, _world: &mut Env) {
        match action {
            Action::TurnOn => println!("on"),
            Action::TurnOff => println!("off"),
        }
    }
}

impl MachineSpec for Light {
    type Domain = Light;
    type Tag = Tag;
}

static STATES: &[State<Light>] = &[
    State { tag: Tag::Off, entry: &[Action::TurnOff], exit: &[] },
    State { tag: Tag::On,  entry: &[Action::TurnOn],  exit: &[] },
];

static EDGES: &[Edge<Light>] = &[
    Edge { id: "TURN_ON",  from: Source::These(&[Tag::Off]), when: Kind::Toggle,
           check: chart::check!(), unknown: OnUnknown::Deny, run: &[], goto: Goto::To(Tag::On) },
    Edge { id: "TURN_OFF", from: Source::These(&[Tag::On]),  when: Kind::Toggle,
           check: chart::check!(), unknown: OnUnknown::Deny, run: &[], goto: Goto::To(Tag::Off) },
];

static IGNORES: &[Ignore<Light>] = &[];

fn main() {
    let mut world = Env;
    let mut m = Machine::new(Tag::Off, STATES, EDGES, IGNORES);
    m.dispatch(&Event::Toggle, &mut world); // -> on
    m.dispatch(&Event::Toggle, &mut world); // -> off
}
```

Bigger examples:
- [`examples/door_lock`](examples/door_lock/main.rs) — four states, guard
  conditions, `Ignore` with wildcard sources. `cargo run --example door_lock`.
- [`examples/mirrors`](examples/mirrors/main.rs) — a controller mixing both
  layers: two stateless features next to one state machine.
  `cargo run --example mirrors`.

## What you get for declaring instead of coding

- **A runtime.** `Machine::dispatch` (or `feature::dispatch`) reads the same
  table you wrote — no separate interpretation step to fall out of sync.
- **A diagram.** `render::to_mermaid` turns the transition table into a
  `stateDiagram-v2` you can drop straight into docs, or convert to PlantUML
  with `scripts/mermaid_to_plantuml.sh`.
- **A gap check.** `render::coverage` walks every `(state, event)`
  combination and reports the ones with no edge and no declared `Ignore`.
  Assert `is_clean()` in a test, and a forgotten case fails CI instead of
  surfacing in production.
- **Guards that admit failure.** Conditions evaluate to `True`/`False`/
  `Unknown` instead of `bool`, and `Edge::unknown` names the fail-open or
  fail-closed policy explicitly — it shows up on the diagram instead of
  hiding inside a guard function.
- **Traceable side effects.** `Domain::perform` is the only place the outside
  world is touched, so every effect a dispatch produced is a plain `Action`
  value you can log or assert on.

## Project layout

```
src/
  lib.rs          // Domain, MachineSpec — library entry points
  feature.rs      // stateless layer: Feature, FeatureInfo, dispatch
  machine.rs      // stateful layer: Machine, Taken
  machine/        // Cond, CondNode, State, Edge, Source, Goto, OnUnknown
  render.rs       // to_mermaid, coverage, io_table, io_flowchart
examples/
  door_lock/      // cargo run --example door_lock
  mirrors/        // cargo run --example mirrors
```

Full API details (the `Domain`/`MachineSpec` contract, guard authoring,
dispatch order, etc.) are documented on the types themselves — run
`cargo doc --open`.

## Tests

```sh
cargo test
```
