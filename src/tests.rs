//! 프레임워크 자체 테스트.
//!
//! README의 "빠른 시작" 절 예제보다 조금 더 복합적인 후방 카메라 컨트롤러를
//! 예제로 프레임워크 전체 기능을 검증한다.

use super::{render, Cond, Domain, Edge, Goto, Ignore, Machine, OnUnknown, Source, StateNode};

// ─────────────────────────────────────────── 도메인 정의

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Tag {
    Off,
    Showing,
}

#[derive(Clone, Debug)]
enum Event {
    GearChanged(Gear),
    SpeedChanged,
    PowerChanged,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Kind {
    GearChanged,
    SpeedChanged,
    PowerChanged,
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
    /// `None`이면 조회 실패 — `Cond::Unknown`을 유발한다.
    speed: Option<f32>,
    camera_visible: bool,
    performed: Vec<Action>,
}

struct RearCam;

impl Domain for RearCam {
    type Tag = Tag;
    type Event = Event;
    type EventKind = Kind;
    type Action = Action;
    type Env = Env;

    fn kind(ev: &Event) -> Kind {
        match ev {
            Event::GearChanged(_) => Kind::GearChanged,
            Event::SpeedChanged => Kind::SpeedChanged,
            Event::PowerChanged => Kind::PowerChanged,
        }
    }

    fn perform(action: Action, _state: &dyn StateNode<Self>, world: &mut Env) {
        world.performed.push(action);
        match action {
            Action::ShowCamera => world.camera_visible = true,
            Action::HideCamera => world.camera_visible = false,
            Action::UpdateOverlay => {}
        }
    }

    fn all_tags() -> &'static [Tag] {
        &[Tag::Off, Tag::Showing]
    }

    fn all_kinds() -> &'static [Kind] {
        &[Kind::GearChanged, Kind::SpeedChanged, Kind::PowerChanged]
    }
}

// ─────────────────────────────────────────── 조건 노드
// payload 노드: 순수. Unknown이 나올 수 없다.

crate::cond_node!(RearCam, GearIsReverse, |cx| match cx.event {
    Event::GearChanged(g) => Cond::from(*g == Gear::Reverse),
    _ => Cond::False,
});

// context 노드: 조회 실패 시 Unknown.
crate::cond_node!(RearCam, SpeedBelowLimit, |cx| match cx.world.speed {
    Some(v) => Cond::from(v < 15.0),
    None => Cond::Unknown,
});

// ─────────────────────────────────────────── 상태

crate::state!(RearCam, Off, tag: Tag::Off);
crate::state!(RearCam, Showing, tag: Tag::Showing,
              on_enter: [Action::ShowCamera],
              on_exit:  [Action::HideCamera]);

// ─────────────────────────────────────────── 표

static EDGES: &[Edge<RearCam>] = &[
    Edge {
        id: "CAM_ON",
        from: Source::These(&[Tag::Off]),
        when: Kind::GearChanged,
        check: crate::check!(GearIsReverse && SpeedBelowLimit),
        unknown: OnUnknown::Deny, // 속도를 모르면 켜지 않는다
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
        unknown: OnUnknown::Allow, // 속도를 모르면 끈다
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
        goto: Goto::Internal, // 상태 불변 — exit/enter 안 돎
    },
];

static IGNORES: &[Ignore<RearCam>] = &[
    Ignore {
        from: Source::These(&[Tag::Off]),
        when: &[Kind::SpeedChanged],
        why: "표시 중이 아니면 속도는 무관",
    },
    Ignore {
        from: Source::Any,
        when: &[Kind::PowerChanged],
        why: "전원은 상위 컨트롤러가 처리",
    },
];

fn machine() -> Machine<RearCam> {
    Machine::new(
        Tag::Off,
        vec![Box::new(Off), Box::new(Showing)],
        EDGES,
        IGNORES,
    )
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
    (m, w)
}

// ─────────────────────────────────────────── 테스트

#[test]
fn enters_showing_and_runs_entry_action() {
    let mut m = machine();
    let mut w = Env {
        speed: Some(10.0),
        ..Default::default()
    };

    let taken = m.dispatch(&Event::GearChanged(Gear::Reverse), &mut w).unwrap();

    assert_eq!(taken.edge, "CAM_ON");
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
fn internal_transition_skips_exit_and_entry() {
    let (mut m, mut w) = showing();

    let taken = m.dispatch(&Event::SpeedChanged, &mut w).unwrap();

    assert_eq!(taken.edge, "CAM_OVERLAY");
    assert_eq!(m.tag(), Tag::Showing);
    // HideCamera / ShowCamera 가 끼지 않는다
    assert_eq!(w.performed, vec![Action::UpdateOverlay]);
}

#[test]
fn unknown_denies_transition_when_policy_is_deny() {
    let mut m = machine();
    let mut w = Env {
        speed: None, // 조회 실패 → SpeedBelowLimit = Unknown
        ..Default::default()
    };

    assert!(m.dispatch(&Event::GearChanged(Gear::Reverse), &mut w).is_none());
    assert_eq!(m.tag(), Tag::Off);
    assert!(w.performed.is_empty());
}

#[test]
fn unknown_allows_transition_when_policy_is_allow() {
    let (mut m, mut w) = showing();

    w.speed = None; // !SpeedBelowLimit = Unknown, 정책은 Allow
    let taken = m.dispatch(&Event::SpeedChanged, &mut w).unwrap();

    assert_eq!(taken.edge, "CAM_OFF_SPEED");
    assert_eq!(m.tag(), Tag::Off);
}

#[test]
fn declaration_order_is_priority() {
    // (Showing, SpeedChanged) 에는 CAM_OFF_SPEED 와 CAM_OVERLAY 가 모두 걸린다.
    // 조건이 상호배타이므로 실제 충돌은 없지만, 순서가 우선순위임을 고정한다.
    let (mut m, mut w) = showing();

    w.speed = Some(20.0); // !SpeedBelowLimit = True → 먼저 선언된 CAM_OFF_SPEED
    assert_eq!(m.dispatch(&Event::SpeedChanged, &mut w).unwrap().edge, "CAM_OFF_SPEED");
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

    // 상호배타 조건이라도 겹침은 리뷰 대상으로 보고된다
    assert_eq!(c.overlaps.len(), 1);
    assert_eq!(c.overlaps[0].2, vec!["CAM_OFF_SPEED", "CAM_OVERLAY"]);
}

#[test]
fn mermaid_matches_golden() {
    let expected = "\
stateDiagram-v2
    [*] --> Off
    Off --> Showing: GearChanged<br/>[GearIsReverse && SpeedBelowLimit]
    Showing --> Off: GearChanged<br/>[!GearIsReverse]
    Showing --> Off: SpeedChanged<br/>[!SpeedBelowLimit]<br/>unknown=Allow
";

    assert_eq!(render::to_mermaid::<RearCam>(Tag::Off, EDGES), expected);
}

#[test]
fn internal_table_lists_state_preserving_edges() {
    let table = render::internal_table::<RearCam>(EDGES);

    assert!(table.contains("CAM_OVERLAY"), "{table}");
    assert!(table.contains("UpdateOverlay"), "{table}");
    // 상태를 바꾸는 엣지는 이 표에 없다
    assert!(!table.contains("CAM_ON"), "{table}");
}

#[test]
fn pump_drains_queue_in_order() {
    let mut m = machine();
    let mut w = Env {
        speed: Some(10.0),
        ..Default::default()
    };

    // post 순서대로, 전이 하나가 완전히 끝난 뒤에만 다음 이벤트가 처리된다.
    m.post(Event::GearChanged(Gear::Reverse));
    m.post(Event::SpeedChanged);
    m.pump(&mut w);

    assert_eq!(m.tag(), Tag::Showing);
    assert_eq!(w.performed, vec![Action::ShowCamera, Action::UpdateOverlay]);
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
