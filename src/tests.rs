//! Tests for the framework itself.
//!
//! Uses a reverse-camera controller, slightly richer than the README quick-start
//! example, to exercise every feature.

// The fixture's event names read better with the shared `Changed` suffix, and
// `events!` emits the same variant names for both enums.
#![allow(clippy::enum_variant_names)]

use super::{
    Cond, Domain, Edge, Expr, Goto, HasKind, Ignore, Machine, OnUnknown, Source, State, render,
};

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

#[derive(Default)]
struct Env {
    /// `None` models a failed lookup, which yields `Cond::Unknown`.
    speed: Option<f32>,
    camera_visible: bool,
    performed: Vec<Action>,
    /// The event kind each action was performed for, recorded from `perform`'s
    /// `ev` argument.
    performed_for: Vec<Kind>,
}

struct RearCam;

impl Domain for RearCam {
    type Tag = Tag;
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

// ─────────────────────────────────────────── guards
// A payload guard: pure, and can never be Unknown.

crate::cond_node!(RearCam, GearIsReverse, |cx| match cx.event {
    Event::GearChanged(g) => Cond::from(*g == Gear::Reverse),
    _ => Cond::False,
});

// A context guard: Unknown when the lookup fails.
crate::cond_node!(RearCam, SpeedBelowLimit, |cx| match cx.world.speed {
    Some(v) => Cond::from(v < 15.0),
    None => Cond::Unknown,
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
