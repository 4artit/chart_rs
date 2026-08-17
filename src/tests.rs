//! Tests for the framework itself.
//!
//! Uses a reverse-camera controller, slightly richer than the README quick-start
//! example, to exercise every feature.

// The fixture's event names read better with the shared `Changed` suffix, and
// `events!` emits the same variant names for both enums.
#![allow(clippy::enum_variant_names)]

use std::cell::Cell;

use super::feature::{self, Feature, FeatureInfo};
use super::machine::{Cond, Cx, Edge, Expr, Goto, Ignore, Machine, Memo, OnUnknown, Source, State};
use super::{Domain, Enumerable, HasKind, MachineSpec, render};

// ─────────────────────────────────────────── domain

crate::tags! {
    enum Tag {
        Off,
        Showing,
    }
}

crate::events! {
    #[derive(Clone, Debug)]
    enum Event => Kind {
        GearChanged(Gear),
        SpeedChanged,
        PowerChanged,
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Gear {
    Reverse,
    Drive,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Action {
    ShowCamera,
    HideCamera,
    UpdateOverlay,
}

// Only needed by `feature::unemitted_actions`; `Domain` does not require it.
impl Enumerable for Action {
    const ALL: &'static [Self] = &[Self::ShowCamera, Self::HideCamera, Self::UpdateOverlay];
}

#[derive(Default)]
struct Env {
    /// `None` models a failed lookup, which yields `Cond::Unknown`.
    speed: Option<f32>,
    camera_visible: bool,
    performed: Vec<Action>,
    /// The event kind each action was performed for, recorded from `perform`'s
    /// `ev` argument.
    performed_for: Vec<Kind>,
    /// How many times `SpeedBelowLimit` looked the speed up. Guards receive
    /// `&Env`, so this needs interior mutability.
    speed_lookups: Cell<u32>,
}

struct RearCam;

impl Domain for RearCam {
    type Event = Event;
    type EventKind = Kind;
    type Action = Action;
    type Env = Env;

    fn perform(action: Action, ev: &Event, world: &mut Env) {
        world.performed.push(action);
        world.performed_for.push(ev.kind());
        match action {
            Action::ShowCamera => world.camera_visible = true,
            Action::HideCamera => world.camera_visible = false,
            Action::UpdateOverlay => {}
        }
    }
}

impl MachineSpec for RearCam {
    type Domain = RearCam;
    type Tag = Tag;
}

// ─────────────────────────────────────────── guards
// A payload guard: pure, and can never be Unknown.

crate::cond_node!(RearCam, GearIsReverse, |cx| match cx.event {
    Event::GearChanged(g) => Cond::from(*g == Gear::Reverse),
    _ => Cond::False,
});

// A context guard: Unknown when the lookup fails. Counts its lookups so that
// short-circuiting is observable.
crate::cond_node!(RearCam, SpeedBelowLimit, |cx| {
    cx.world.speed_lookups.set(cx.world.speed_lookups.get() + 1);
    match cx.world.speed {
        Some(v) => Cond::from(v < 15.0),
        None => Cond::Unknown,
    }
});

// ─────────────────────────────────────────── states

static STATES: &[State<RearCam>] = &[
    State {
        tag: Tag::Off,
        entry: &[],
        exit: &[],
    },
    State {
        tag: Tag::Showing,
        entry: &[Action::ShowCamera],
        exit: &[Action::HideCamera],
    },
];

// ─────────────────────────────────────────── table

static EDGES: &[Edge<RearCam>] = &[
    Edge {
        id: "CAM_ON",
        from: Source::These(&[Tag::Off]),
        when: Kind::GearChanged,
        check: crate::check!(GearIsReverse && SpeedBelowLimit),
        unknown: OnUnknown::Deny, // unknown speed: do not turn it on
        run: &[],
        goto: Goto::To(Tag::Showing),
    },
    Edge {
        id: "CAM_OFF_GEAR",
        from: Source::These(&[Tag::Showing]),
        when: Kind::GearChanged,
        check: crate::check!(!GearIsReverse),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(Tag::Off),
    },
    Edge {
        id: "CAM_OFF_SPEED",
        from: Source::These(&[Tag::Showing]),
        when: Kind::SpeedChanged,
        check: crate::check!(!SpeedBelowLimit),
        unknown: OnUnknown::Allow, // unknown speed: turn it off
        run: &[],
        goto: Goto::To(Tag::Off),
    },
    Edge {
        id: "CAM_OVERLAY",
        from: Source::These(&[Tag::Showing]),
        when: Kind::SpeedChanged,
        check: crate::check!(SpeedBelowLimit),
        unknown: OnUnknown::Deny,
        run: &[Action::UpdateOverlay],
        goto: Goto::Internal, // stays in state, so exit/enter do not run
    },
];

static IGNORES: &[Ignore<RearCam>] = &[
    Ignore {
        from: Source::These(&[Tag::Off]),
        when: &[Kind::SpeedChanged],
        why: "speed is irrelevant while the camera is not shown",
    },
    Ignore {
        from: Source::Any,
        when: &[Kind::PowerChanged],
        why: "power is handled by the parent controller",
    },
];

fn machine() -> Machine<RearCam> {
    Machine::new(Tag::Off, STATES, EDGES, IGNORES)
}

fn showing() -> (Machine<RearCam>, Env) {
    let mut m = machine();
    let mut w = Env {
        speed: Some(10.0),
        ..Default::default()
    };
    m.dispatch(&Event::GearChanged(Gear::Reverse), &mut w);
    assert_eq!(m.tag(), Tag::Showing);
    w.performed.clear();
    w.performed_for.clear();
    (m, w)
}

// ─────────────────────────────────────────── tests

#[test]
fn enters_showing_and_runs_entry_action() {
    let mut m = machine();
    let mut w = Env {
        speed: Some(10.0),
        ..Default::default()
    };

    let taken = m
        .dispatch(&Event::GearChanged(Gear::Reverse), &mut w)
        .unwrap();

    assert_eq!(taken.edge, "CAM_ON");
    assert_eq!(taken.actions, vec![Action::ShowCamera]);
    assert_eq!(m.tag(), Tag::Showing);
    assert_eq!(w.performed, vec![Action::ShowCamera]);
    assert!(w.camera_visible);
}

/// `Taken`'s impls are hand-written so that they bound `D::Action` rather than
/// `D`, which a derive would have required.
#[test]
fn taken_is_debug_clone_and_eq() {
    let mut m = machine();
    let mut w = Env {
        speed: Some(10.0),
        ..Default::default()
    };

    let on = m
        .dispatch(&Event::GearChanged(Gear::Reverse), &mut w)
        .unwrap();

    assert_eq!(
        format!("{on:?}"),
        r#"Taken { edge: "CAM_ON", actions: [ShowCamera] }"#
    );
    assert_eq!(on.clone(), on);

    let off = m
        .dispatch(&Event::GearChanged(Gear::Drive), &mut w)
        .unwrap();
    assert_ne!(off, on);
}

#[test]
fn exit_action_runs_on_leaving() {
    let (mut m, mut w) = showing();

    m.dispatch(&Event::GearChanged(Gear::Drive), &mut w);

    assert_eq!(m.tag(), Tag::Off);
    assert_eq!(w.performed, vec![Action::HideCamera]);
    assert!(!w.camera_visible);
}

#[test]
fn exit_and_entry_actions_run_in_order_across_a_round_trip() {
    let mut m = machine();
    let mut w = Env {
        speed: Some(10.0),
        ..Default::default()
    };

    m.dispatch(&Event::GearChanged(Gear::Reverse), &mut w);
    m.dispatch(&Event::GearChanged(Gear::Drive), &mut w);

    assert_eq!(m.tag(), Tag::Off);
    assert_eq!(w.performed, vec![Action::ShowCamera, Action::HideCamera]);
    assert!(!w.camera_visible);
}

#[test]
fn internal_transition_skips_exit_and_entry() {
    let (mut m, mut w) = showing();

    let taken = m.dispatch(&Event::SpeedChanged, &mut w).unwrap();

    assert_eq!(taken.edge, "CAM_OVERLAY");
    assert_eq!(m.tag(), Tag::Showing);
    // Neither HideCamera nor ShowCamera slips in.
    assert_eq!(w.performed, vec![Action::UpdateOverlay]);
}

#[test]
fn unknown_denies_transition_when_policy_is_deny() {
    let mut m = machine();
    let mut w = Env {
        speed: None, // failed lookup -> SpeedBelowLimit = Unknown
        ..Default::default()
    };

    assert!(
        m.dispatch(&Event::GearChanged(Gear::Reverse), &mut w)
            .is_none()
    );
    assert_eq!(m.tag(), Tag::Off);
    assert!(w.performed.is_empty());
}

#[test]
fn unknown_allows_transition_when_policy_is_allow() {
    let (mut m, mut w) = showing();

    w.speed = None; // !SpeedBelowLimit = Unknown, and the policy is Allow
    let taken = m.dispatch(&Event::SpeedChanged, &mut w).unwrap();

    assert_eq!(taken.edge, "CAM_OFF_SPEED");
    assert_eq!(m.tag(), Tag::Off);
}

#[test]
fn declaration_order_is_priority() {
    // Both CAM_OFF_SPEED and CAM_OVERLAY match (Showing, SpeedChanged). Their
    // guards are mutually exclusive, so there is no real conflict; this pins down
    // that declaration order decides.
    let (mut m, mut w) = showing();

    w.speed = Some(20.0); // !SpeedBelowLimit = True -> the earlier CAM_OFF_SPEED
    assert_eq!(
        m.dispatch(&Event::SpeedChanged, &mut w).unwrap().edge,
        "CAM_OFF_SPEED"
    );
}

#[test]
fn declared_ignore_is_not_a_hole() {
    let mut m = machine();
    let mut w = Env::default();

    assert!(m.dispatch(&Event::PowerChanged, &mut w).is_none());
    assert_eq!(m.tag(), Tag::Off);
}

#[test]
fn coverage_has_no_holes_and_no_unreachable_state() {
    let c = render::coverage::<RearCam>(Tag::Off, EDGES, IGNORES);

    assert!(c.holes.is_empty(), "holes: {:?}", c.holes);
    assert!(c.unreachable.is_empty(), "unreachable: {:?}", c.unreachable);
    assert!(c.is_clean());

    // Overlaps are reported even when the guards are mutually exclusive.
    assert_eq!(c.overlaps.len(), 1);
    assert_eq!(c.overlaps[0].2, vec!["CAM_OFF_SPEED", "CAM_OVERLAY"]);
}

#[test]
fn mermaid_matches_golden() {
    let expected = "\
stateDiagram-v2
    [*] --> Off
    Showing : Showing<br/>entry / ShowCamera<br/>exit / HideCamera
    Off --> Showing: GearChanged<br/>[GearIsReverse && SpeedBelowLimit]
    Showing --> Off: GearChanged<br/>[!GearIsReverse]
    Showing --> Off: SpeedChanged<br/>[!SpeedBelowLimit]<br/>unknown=Allow
";

    // No machine needed: the diagram comes from the static tables alone.
    assert_eq!(
        render::to_mermaid::<RearCam>(Tag::Off, EDGES, STATES),
        expected
    );
}

#[test]
fn internal_table_lists_state_preserving_edges() {
    let table = render::internal_table::<RearCam>(EDGES);

    assert!(table.contains("CAM_OVERLAY"), "{table}");
    assert!(table.contains("UpdateOverlay"), "{table}");
    // Edges that change state are not in this table.
    assert!(!table.contains("CAM_ON"), "{table}");
}

#[test]
fn caller_side_queue_processes_events_in_order() {
    use std::collections::VecDeque;

    let mut m = machine();
    let mut w = Env {
        speed: Some(10.0),
        ..Default::default()
    };

    // Machine owns no queue. Driving it from the caller exposes each Taken.
    let mut pending = VecDeque::from([Event::GearChanged(Gear::Reverse), Event::SpeedChanged]);
    let mut taken = Vec::new();
    while let Some(ev) = pending.pop_front() {
        if let Some(t) = m.dispatch(&ev, &mut w) {
            taken.push(t.edge);
        }
    }

    assert_eq!(taken, vec!["CAM_ON", "CAM_OVERLAY"]);
    assert_eq!(m.tag(), Tag::Showing);
    assert_eq!(w.performed, vec![Action::ShowCamera, Action::UpdateOverlay]);
}

#[test]
fn perform_sees_the_event_on_an_internal_transition() {
    let (mut m, mut w) = showing();

    // CAM_OVERLAY is Goto::Internal, so neither on_enter nor on_exit runs. Its
    // action can still read the event, which is the only route to a payload here
    // because Edge::run holds compile-time constants only.
    let taken = m.dispatch(&Event::SpeedChanged, &mut w).unwrap();

    assert_eq!(taken.edge, "CAM_OVERLAY");
    assert_eq!(w.performed, vec![Action::UpdateOverlay]);
    assert_eq!(w.performed_for, vec![Kind::SpeedChanged]);
}

#[test]
fn perform_sees_the_event_for_entry_and_exit_actions() {
    let (mut m, mut w) = showing();

    // on_exit's actions are performed after the transition, but still for the
    // event that caused it.
    m.dispatch(&Event::GearChanged(Gear::Drive), &mut w);

    assert_eq!(w.performed, vec![Action::HideCamera]);
    assert_eq!(w.performed_for, vec![Kind::GearChanged]);
}

#[test]
fn ignore_table_lists_reasons() {
    let table = render::ignore_table::<RearCam>(IGNORES);

    assert!(table.contains("speed is irrelevant"), "{table}");
    assert!(table.contains("power is handled"), "{table}");
    // Source::Any is expanded into concrete states.
    assert!(table.contains("`Off` | `PowerChanged`"), "{table}");
    assert!(table.contains("`Showing` | `PowerChanged`"), "{table}");
}

#[test]
fn render_parenthesises_negated_subexpressions() {
    // check! only negates single nodes, but Expr can be built by hand (the
    // documented route for `||`), and then precedence must survive rendering.
    static NEGATED_AND: Expr<RearCam> = Expr::Not(&Expr::And(
        &Expr::Node(&GearIsReverse),
        &Expr::Node(&SpeedBelowLimit),
    ));

    assert_eq!(NEGATED_AND.render(), "!(GearIsReverse && SpeedBelowLimit)");
    // A negated single node needs no parentheses.
    assert_eq!(crate::check!(!GearIsReverse).render(), "!GearIsReverse");
}

/// `check!()` with no arguments. An edge with no guard is always taken.
#[test]
fn always_is_true_renders_empty_and_references_no_nodes() {
    let w = Env::default();
    let ev = Event::PowerChanged;
    let memo = Memo::new();
    let cx: Cx<'_, RearCam> = Cx::new(&ev, &w, &memo);

    let always: &Expr<RearCam> = crate::check!();

    assert_eq!(always.eval(&cx), Cond::True);
    assert_eq!(always.render(), "");

    let mut ids = Vec::new();
    always.node_ids(&mut ids);
    assert!(ids.is_empty());
}

/// `||` has no macro form, so `Expr::Or` is built by hand.
#[test]
fn or_short_circuits_on_true() {
    static EITHER: Expr<RearCam> =
        Expr::Or(&Expr::Node(&GearIsReverse), &Expr::Node(&SpeedBelowLimit));

    assert_eq!(EITHER.render(), "(GearIsReverse || SpeedBelowLimit)");

    let mut ids = Vec::new();
    EITHER.node_ids(&mut ids);
    assert_eq!(ids.len(), 2);

    // Left is True, so the speed is never looked up.
    let w = Env {
        speed: None,
        ..Default::default()
    };
    let ev = Event::GearChanged(Gear::Reverse);
    let memo = Memo::new();
    assert_eq!(EITHER.eval(&Cx::new(&ev, &w, &memo)), Cond::True);
    assert_eq!(w.speed_lookups.get(), 0);

    // Left is False, so the right operand decides — and its lookup fails.
    let ev = Event::GearChanged(Gear::Drive);
    let memo = Memo::new();
    assert_eq!(EITHER.eval(&Cx::new(&ev, &w, &memo)), Cond::Unknown);
    assert_eq!(w.speed_lookups.get(), 1);
}

/// A `False` on the left settles an `And`, so the right operand is skipped.
#[test]
fn and_short_circuits_on_false() {
    let w = Env {
        speed: None,
        ..Default::default()
    };
    let ev = Event::GearChanged(Gear::Drive); // GearIsReverse -> False
    let memo = Memo::new();

    let both = crate::check!(GearIsReverse && SpeedBelowLimit);

    assert_eq!(both.eval(&Cx::new(&ev, &w, &memo)), Cond::False);
    assert_eq!(w.speed_lookups.get(), 0);
}

#[test]
fn macros_generate_exhaustive_lists() {
    use crate::Enumerable;

    // The macros supply what all_tags/all_kinds used to spell out by hand.
    assert_eq!(RearCam::all_tags(), &[Tag::Off, Tag::Showing]);
    assert_eq!(
        RearCam::all_kinds(),
        &[Kind::GearChanged, Kind::SpeedChanged, Kind::PowerChanged]
    );
    assert_eq!(Tag::ALL.len(), 2);
    assert_eq!(Kind::ALL.len(), 3);
}

#[test]
fn generated_kind_maps_payload_and_unit_variants() {
    use crate::HasKind;

    // Payload-carrying and unit variants expand under the same rule.
    assert_eq!(Event::GearChanged(Gear::Reverse).kind(), Kind::GearChanged);
    assert_eq!(Event::GearChanged(Gear::Drive).kind(), Kind::GearChanged);
    assert_eq!(Event::SpeedChanged.kind(), Kind::SpeedChanged);
    assert_eq!(Event::PowerChanged.kind(), Kind::PowerChanged);
}

#[test]
fn source_any_except_matches_all_but_listed() {
    let s = Source::<RearCam>::AnyExcept(&[Tag::Showing]);

    assert!(s.matches(Tag::Off));
    assert!(!s.matches(Tag::Showing));
}

#[test]
fn source_any_matches_every_tag() {
    let s = Source::<RearCam>::Any;

    assert!(s.matches(Tag::Off));
    assert!(s.matches(Tag::Showing));
}

#[test]
fn ignore_any_except_matches_multiple_tags() {
    let ignore = Ignore::<RearCam> {
        from: Source::AnyExcept(&[Tag::Showing]),
        when: &[Kind::PowerChanged],
        why: "test",
    };

    assert!(ignore.matches(Tag::Off, Kind::PowerChanged));
    assert!(!ignore.matches(Tag::Showing, Kind::PowerChanged));
    assert!(!ignore.matches(Tag::Off, Kind::GearChanged));
}

// ─────────────────────────────────────────── narrowed domain
// `Domain::all_tags` may be overridden to check a subset, which takes the excluded
// tags out of the first validation loop. Edge targets are checked separately.

struct PartialCam;

impl Domain for PartialCam {
    type Event = Event;
    type EventKind = Kind;
    type Action = Action;
    type Env = Env;

    fn perform(_action: Action, _ev: &Event, _world: &mut Env) {}
}

impl MachineSpec for PartialCam {
    type Domain = PartialCam;
    type Tag = Tag;

    fn all_tags() -> &'static [Tag] {
        &[Tag::Off]
    }
}

static PARTIAL_STATES: &[State<PartialCam>] = &[State {
    tag: Tag::Off,
    entry: &[],
    exit: &[],
}];

static PARTIAL_EDGES: &[Edge<PartialCam>] = &[Edge {
    id: "TO_UNDECLARED",
    from: Source::These(&[Tag::Off]),
    when: Kind::GearChanged,
    check: crate::check!(),
    unknown: OnUnknown::Deny,
    run: &[],
    goto: Goto::To(Tag::Showing), // absent from all_tags and from PARTIAL_STATES
}];

static PARTIAL_IGNORES: &[Ignore<PartialCam>] = &[Ignore {
    from: Source::Any,
    when: &[Kind::SpeedChanged, Kind::PowerChanged],
    why: "outside this fixture",
}];

/// A `Goto::To` pointing outside the state table is rejected at construction, not
/// when the transition is eventually taken.
#[test]
#[should_panic(expected = "edge TO_UNDECLARED goes to Showing")]
fn an_edge_targeting_a_tag_outside_the_state_table_is_rejected() {
    let _ = Machine::new(Tag::Off, PARTIAL_STATES, PARTIAL_EDGES, PARTIAL_IGNORES);
}

// ─────────────────────────────────────────── defective table
// RearCam is deliberately clean, so the diagnostics never fire on it. This table
// trips each of them: a hole, an unreachable state, and one guard name shared by
// three node types.

struct Broken;

impl Domain for Broken {
    type Event = Event;
    type EventKind = Kind;
    type Action = Action;
    type Env = Env;

    fn perform(_action: Action, _ev: &Event, _world: &mut Env) {}
}

impl MachineSpec for Broken {
    type Domain = Broken;
    type Tag = Tag;
}

crate::cond_node!(Broken, Duplicate, |_cx| Cond::True);

struct AlsoDuplicate;
struct StillDuplicate;

impl crate::machine::CondNode<Broken> for AlsoDuplicate {
    fn name(&self) -> &'static str {
        "Duplicate"
    }
    fn eval(&self, _cx: &Cx<'_, Broken>) -> Cond {
        Cond::True
    }
}

impl crate::machine::CondNode<Broken> for StillDuplicate {
    fn name(&self) -> &'static str {
        "Duplicate"
    }
    fn eval(&self, _cx: &Cx<'_, Broken>) -> Cond {
        Cond::True
    }
}

/// `Showing` is missing, so `to_mermaid` has no description to draw for it.
static BROKEN_STATES: &[State<Broken>] = &[State {
    tag: Tag::Off,
    entry: &[],
    exit: &[],
}];

static BROKEN_EDGES: &[Edge<Broken>] = &[
    Edge {
        id: "NO_GUARD",
        from: Source::These(&[Tag::Off]),
        when: Kind::GearChanged,
        check: crate::check!(),
        unknown: OnUnknown::Deny,
        run: &[Action::UpdateOverlay],
        goto: Goto::To(Tag::Off), // nothing reaches Showing
    },
    Edge {
        id: "DUPED_NAMES",
        from: Source::These(&[Tag::Off]),
        when: Kind::SpeedChanged,
        check: &Expr::And(
            &Expr::Node(&Duplicate),
            &Expr::And(&Expr::Node(&AlsoDuplicate), &Expr::Node(&StillDuplicate)),
        ),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(Tag::Off),
    },
    Edge {
        id: "SILENT_INTERNAL",
        from: Source::These(&[Tag::Off]),
        when: Kind::PowerChanged,
        check: crate::check!(),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::Internal,
    },
];

#[test]
fn coverage_reports_holes_unreachable_states_and_duplicate_names() {
    let c = render::coverage::<Broken>(Tag::Off, BROKEN_EDGES, &[]);

    assert!(!c.is_clean());
    // Nothing is declared for Showing at all.
    assert!(
        c.holes
            .contains(&("Showing".to_owned(), "GearChanged".to_owned())),
        "{:?}",
        c.holes
    );
    assert_eq!(c.unreachable, vec!["Showing"]);
    // Reported once, however many types share the name.
    assert_eq!(c.duplicate_node_names, vec!["Duplicate"]);
}

#[test]
fn mermaid_labels_a_guardless_edge_and_its_run_actions() {
    let diagram = render::to_mermaid::<Broken>(Tag::Off, BROKEN_EDGES, BROKEN_STATES);

    assert!(
        diagram.contains("Off --> Off: GearChanged<br/>/ UpdateOverlay"),
        "{diagram}"
    );
    // Showing has no state table entry, so it gets no description line.
    assert!(!diagram.contains("Showing :"), "{diagram}");
}

#[test]
fn internal_table_dashes_an_empty_guard() {
    let table = render::internal_table::<Broken>(BROKEN_EDGES);

    assert!(table.contains("`SILENT_INTERNAL`"), "{table}");
    assert!(table.contains("`—`"), "{table}");
}

// ─────────────────────────────────────────── feature layer
// The same domain, driven without a transition table. `RearCam` has states, but
// nothing about `Feature` requires them — it only needs `Domain`.

struct Camera;

impl Feature<RearCam> for Camera {
    const INFO: FeatureInfo<RearCam> = FeatureInfo {
        name: "Camera",
        handles: &[Kind::GearChanged],
        emits: &[Action::ShowCamera, Action::HideCamera],
    };

    fn handle(&mut self, ev: &Event, _world: &Env, out: &mut Vec<Action>) {
        if let Event::GearChanged(g) = ev {
            out.push(if *g == Gear::Reverse {
                Action::ShowCamera
            } else {
                Action::HideCamera
            });
        }
    }
}

struct Overlay;

impl Feature<RearCam> for Overlay {
    const INFO: FeatureInfo<RearCam> = FeatureInfo {
        name: "Overlay",
        handles: &[Kind::SpeedChanged],
        emits: &[Action::UpdateOverlay],
    };

    fn handle(&mut self, _ev: &Event, world: &Env, out: &mut Vec<Action>) {
        if world.speed.is_some() {
            out.push(Action::UpdateOverlay);
        }
    }
}

static CAMERA_FEATURES: &[FeatureInfo<RearCam>] = &[Camera::INFO, Overlay::INFO];

#[test]
fn dispatch_runs_only_the_declared_kinds() {
    let mut cam = Camera;
    let mut w = Env::default();
    let mut out = Vec::new();

    // Declared: reaches the handler.
    feature::dispatch(&mut cam, &Event::GearChanged(Gear::Reverse), &w, &mut out);
    assert_eq!(out, vec![Action::ShowCamera]);

    // Not declared: the handler never runs, so nothing is emitted.
    w.speed = Some(10.0);
    feature::dispatch(&mut cam, &Event::SpeedChanged, &w, &mut out);
    assert_eq!(out, vec![Action::ShowCamera]);
}

#[test]
fn io_table_lists_each_feature() {
    let table = render::io_table(CAMERA_FEATURES);

    assert!(
        table.contains("| `Camera` | `GearChanged` | `ShowCamera`, `HideCamera` |"),
        "{table}"
    );
    assert!(
        table.contains("| `Overlay` | `SpeedChanged` | `UpdateOverlay` |"),
        "{table}"
    );
}

#[test]
fn io_flowchart_keeps_features_and_actions_apart() {
    let chart = render::io_flowchart(CAMERA_FEATURES);

    assert!(chart.contains(r#"ev_GearChanged["GearChanged"] --> ft_Camera["Camera"]"#));
    assert!(chart.contains(r#"ft_Camera["Camera"] --> ac_ShowCamera["ShowCamera"]"#));
}

#[test]
fn unhandled_kinds_reports_what_no_feature_takes() {
    // PowerChanged is in the event enum but no feature declares it.
    assert_eq!(
        feature::unhandled_kinds(CAMERA_FEATURES, &[]),
        vec![Kind::PowerChanged]
    );
}

/// A controller that mixes both layers is checked as one unit. An `Ignore` does
/// not make a kind handled — the table only says the machine has no use for it.
#[test]
fn unhandled_kinds_counts_edges_but_not_ignores() {
    let by_machine = render::handled_kinds::<RearCam>(EDGES);

    assert!(by_machine.contains(&Kind::GearChanged));
    assert!(
        !by_machine.contains(&Kind::PowerChanged),
        "PowerChanged only has an Ignore"
    );
    assert_eq!(
        feature::unhandled_kinds(CAMERA_FEATURES, &[&by_machine]),
        vec![Kind::PowerChanged]
    );
}

#[test]
fn unemitted_actions_reports_what_no_feature_produces() {
    let only_camera = &[Camera::INFO];

    assert_eq!(
        feature::unemitted_actions(only_camera),
        vec![Action::UpdateOverlay]
    );
}

/// A feature that emits something it did not declare. `dispatch` catches it in
/// debug builds rather than letting the table quietly go stale.
struct Liar;

impl Feature<RearCam> for Liar {
    const INFO: FeatureInfo<RearCam> = FeatureInfo {
        name: "Liar",
        handles: &[Kind::GearChanged],
        emits: &[Action::ShowCamera],
    };

    fn handle(&mut self, _ev: &Event, _world: &Env, out: &mut Vec<Action>) {
        out.push(Action::UpdateOverlay);
    }
}

#[test]
#[should_panic(expected = "Liar: emitted an action it does not declare")]
fn dispatch_rejects_an_undeclared_action() {
    feature::dispatch(
        &mut Liar,
        &Event::GearChanged(Gear::Reverse),
        &Env::default(),
        &mut Vec::new(),
    );
}
