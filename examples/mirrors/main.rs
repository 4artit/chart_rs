//! A side mirror controller split by feature, mixing both layers.
//!
//!     cargo run --example mirrors
//!
//! | File | Layer | Why |
//! |---|---|---|
//! | `heating.rs` | feature | Follows the defog signal |
//! | `dimming.rs` | feature | A function of power and gear |
//! | `fold.rs` | machine | Folding and unfolding are observable states |
//!
//! A ticket about folding is answered by `fold.rs` alone. `handle_event` routes
//! and does not decide.

mod dimming;
mod fold;
mod heating;

use chart::feature::{self, Feature, FeatureInfo};
use chart::{Domain, HasKind, render};

use dimming::Dimming;
use fold::Fold;
use heating::Heating;

chart::events! {
    #[derive(Clone, Debug)]
    pub enum Event => Kind {
        DefogChanged(bool),
        PowerChanged(bool),
        GearChanged(bool),
        SpeedChanged(f32),
        /// Position reported by the fold motor. 0.0 folded, 1.0 unfolded.
        FoldPositionChanged(f32),
        UserChanged,
    }
}

/// Carries no payload, which is what the `machine` layer's `&'static [Action]`
/// requires. Runtime values are read from `ev` or the world inside `perform`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Action {
    HeatingOn,
    HeatingOff,
    DimmingOn,
    DimmingOff,
    Fold,
    Unfold,
}

/// The outside world. Stands in for an API bridge.
#[derive(Default)]
pub struct World {
    pub power_on: bool,
    pub gear_reverse: bool,
    pub speed: f32,
    pub fold_position: f32,
    pub effects: Vec<String>,
}

/// The vocabulary both layers work in.
pub struct Mirrors;

impl Domain for Mirrors {
    type Event = Event;
    type EventKind = Kind;
    type Action = Action;
    type Env = World;

    /// The only place the world is touched. Shared by both layers.
    fn perform(action: Action, _ev: &Event, world: &mut World) {
        let line = match action {
            Action::HeatingOn => "heating on".to_string(),
            Action::HeatingOff => "heating off".to_string(),
            Action::DimmingOn => "dimming on".to_string(),
            Action::DimmingOff => "dimming off".to_string(),
            Action::Fold => format!("fold (speed {:.0})", world.speed),
            Action::Unfold => "unfold".to_string(),
        };
        world.effects.push(line);
    }
}

// ─────────────────────────────────────────── router

/// Kept by hand, next to the router so that a missing entry is visible.
const FEATURES: &[FeatureInfo<Mirrors>] = &[Heating::INFO, Dimming::INFO];

#[derive(Default)]
struct Controller {
    heating: Heating,
    dimming: Dimming,
    fold: Fold,
}

impl Controller {
    fn handle_event(&mut self, ev: &Event, world: &mut World) {
        // The one global gate. It says why it dropped the event.
        if !world.power_on && requires_power(ev.kind()) {
            println!("  (dropped: powered off) {ev:?}");
            return;
        }

        // Stateless layer: actions are collected, then carried out here.
        let mut actions = Vec::new();
        feature::dispatch(&mut self.heating, ev, world, &mut actions);
        feature::dispatch(&mut self.dimming, ev, world, &mut actions);
        for &a in &actions {
            Mirrors::perform(a, ev, world);
        }

        // Stateful layer: the machine carries its own out and reports back.
        let taken = self.fold.dispatch(ev, world);

        match (actions.is_empty(), taken) {
            (true, None) => println!("  -> (nothing)"),
            (false, None) => println!("  -> {actions:?}"),
            (true, Some(t)) => println!("  -> [fold:{}] {:?}", t.edge, t.actions),
            (false, Some(t)) => {
                println!("  -> {actions:?} + [fold:{}] {:?}", t.edge, t.actions)
            }
        }
    }
}

/// Which events need power, as one list rather than a check per handler.
fn requires_power(kind: Kind) -> bool {
    matches!(kind, Kind::DefogChanged)
}

// ─────────────────────────────────────────── run

fn main() {
    let mut c = Controller::default();
    let mut w = World {
        fold_position: 1.0, // starts unfolded
        ..Default::default()
    };

    let steps: &[(&str, Event)] = &[
        ("defog while powered off", Event::DefogChanged(true)),
        ("power on", Event::PowerChanged(true)),
        ("defog on", Event::DefogChanged(true)),
        ("gear to reverse", Event::GearChanged(true)),
        ("power off, folding starts", Event::PowerChanged(false)),
        ("motor halfway", Event::FoldPositionChanged(0.5)),
        ("motor folded", Event::FoldPositionChanged(0.0)),
        ("power on, unfolding starts", Event::PowerChanged(true)),
        ("motor unfolded", Event::FoldPositionChanged(1.0)),
        ("user switched, handled by nobody", Event::UserChanged),
    ];

    println!("── run ──");
    for (desc, ev) in steps {
        println!("{desc}");
        apply_signal(ev, &mut w);
        c.handle_event(ev, &mut w);
    }

    println!("\neffects: {:#?}", w.effects);

    let path = "examples/mirrors/mirrors.md";
    std::fs::write(path, document()).expect("failed to write mirrors.md");
    println!("\nwrote {path}");
}

/// The whole controller, drawn from its declarations.
fn document() -> String {
    // Both layers at once: the machine's events are not holes.
    let by_fold = fold::handled_kinds();
    let unhandled = feature::unhandled_kinds(FEATURES, &[&by_fold]);
    let cov = fold::coverage();

    format!(
        "\
# Mirrors controller

What this controller reacts to and what it does about it. Generated from the
declarations, so it cannot drift from the code — regenerate with
`cargo run --example mirrors`.

## Features

Stateless features, one per file, each declaring what it handles and emits.

{table}
## Events, features and actions

```mermaid
{flow}```

## Folding

Folding and unfolding are observable states, so this one is a state machine.

```mermaid
{diagram}```

## Checks

| Check | Result |
|---|---|
| Events nothing handles | {unhandled:?} |
| Holes in the fold table | {holes:?} |
| Fold table is clean | {clean} |
",
        table = render::io_table(FEATURES),
        flow = render::io_flowchart(FEATURES),
        diagram = fold::diagram(),
        unhandled = unhandled,
        holes = cov.holes,
        clean = cov.is_clean(),
    )
}

/// Applies the value a callback carried. A real service would do this.
fn apply_signal(ev: &Event, w: &mut World) {
    match ev {
        Event::PowerChanged(on) => w.power_on = *on,
        Event::GearChanged(rev) => w.gear_reverse = *rev,
        Event::SpeedChanged(v) => w.speed = *v,
        Event::FoldPositionChanged(p) => w.fold_position = *p,
        _ => {}
    }
}
