//! fsm 크레이트 사용 예제 — 4자리 코드로 여닫는 도어락.
//!
//! 상태 4개(Locked/Unlocked/Alarm/Maintenance), 엣지 7개로 구성된다.

use fsm::{Cond, Domain, Edge, Goto, Ignore, Machine, OnUnknown, Source, StateNode, render};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Tag {
    Locked,
    Unlocked,
    Alarm,
    Maintenance,
}

#[derive(Clone, Debug)]
enum Event {
    EnterCode(u32),
    Timeout,
    Reset,
    MaintenanceToggle,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Kind {
    EnterCode,
    Timeout,
    Reset,
    MaintenanceToggle,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Action {
    Unlock,
    Lock,
    Beep,
    IncrementAttempts,
    ResetAttempts,
    SoundAlarm,
    MaintenanceOn,
    MaintenanceOff,
}

struct Env {
    correct_code: u32,
    attempts: u32,
    max_attempts: u32,
}

struct Door;

impl Domain for Door {
    type Tag = Tag;
    type Event = Event;
    type EventKind = Kind;
    type Action = Action;
    type Env = Env;

    fn kind(ev: &Event) -> Kind {
        match ev {
            Event::EnterCode(_) => Kind::EnterCode,
            Event::Timeout => Kind::Timeout,
            Event::Reset => Kind::Reset,
            Event::MaintenanceToggle => Kind::MaintenanceToggle,
        }
    }

    fn perform(action: Action, _state: &dyn StateNode<Self>, world: &mut Env) {
        match action {
            Action::Unlock => println!("  [action] unlock"),
            Action::Lock => println!("  [action] lock"),
            Action::Beep => println!("  [action] beep (wrong code)"),
            Action::IncrementAttempts => {
                world.attempts += 1;
                println!("  [action] attempts = {}", world.attempts);
            }
            Action::ResetAttempts => {
                world.attempts = 0;
                println!("  [action] attempts reset");
            }
            Action::SoundAlarm => println!("  [action] 🚨 alarm on"),
            Action::MaintenanceOn => println!("  [action] maintenance mode on"),
            Action::MaintenanceOff => println!("  [action] maintenance mode off"),
        }
    }

    fn all_tags() -> &'static [Tag] {
        &[Tag::Locked, Tag::Unlocked, Tag::Alarm, Tag::Maintenance]
    }

    fn all_kinds() -> &'static [Kind] {
        &[
            Kind::EnterCode,
            Kind::Timeout,
            Kind::Reset,
            Kind::MaintenanceToggle,
        ]
    }
}

fsm::cond_node!(Door, CodeCorrect, |cx| match cx.event {
    Event::EnterCode(code) => Cond::from(*code == cx.world.correct_code),
    _ => Cond::False,
});

fsm::cond_node!(Door, AttemptsExceeded, |cx| Cond::from(
    cx.world.attempts >= cx.world.max_attempts
));

fsm::state!(Door, Locked, tag: Tag::Locked,
            on_enter: [Action::Lock, Action::ResetAttempts]);
fsm::state!(Door, Unlocked, tag: Tag::Unlocked, on_enter: [Action::Unlock]);
fsm::state!(Door, Alarm, tag: Tag::Alarm, on_enter: [Action::SoundAlarm]);
fsm::state!(Door, Maintenance, tag: Tag::Maintenance,
            on_enter: [Action::MaintenanceOn], on_exit: [Action::MaintenanceOff]);

static EDGES: &[Edge<Door>] = &[
    Edge {
        id: "UNLOCK",
        from: Source::These(&[Tag::Locked]),
        when: Kind::EnterCode,
        check: fsm::check!(CodeCorrect),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(Tag::Unlocked),
    },
    Edge {
        id: "WRONG_CODE",
        from: Source::These(&[Tag::Locked]),
        when: Kind::EnterCode,
        check: fsm::check!(!CodeCorrect && !AttemptsExceeded),
        unknown: OnUnknown::Deny,
        run: &[Action::Beep, Action::IncrementAttempts],
        goto: Goto::Internal,
    },
    Edge {
        id: "TRIGGER_ALARM",
        from: Source::These(&[Tag::Locked]),
        when: Kind::EnterCode,
        check: fsm::check!(!CodeCorrect && AttemptsExceeded),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(Tag::Alarm),
    },
    Edge {
        id: "RELOCK",
        from: Source::These(&[Tag::Unlocked]),
        when: Kind::Timeout,
        check: fsm::check!(),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(Tag::Locked),
    },
    Edge {
        id: "ALARM_RESET",
        from: Source::These(&[Tag::Alarm]),
        when: Kind::Reset,
        check: fsm::check!(),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(Tag::Locked),
    },
    Edge {
        id: "ENTER_MAINTENANCE",
        from: Source::These(&[Tag::Locked]),
        when: Kind::MaintenanceToggle,
        check: fsm::check!(),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(Tag::Maintenance),
    },
    Edge {
        id: "EXIT_MAINTENANCE",
        from: Source::These(&[Tag::Maintenance]),
        when: Kind::MaintenanceToggle,
        check: fsm::check!(),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(Tag::Locked),
    },
];

static IGNORES: &[Ignore<Door>] = &[
    Ignore {
        from: Source::These(&[Tag::Locked, Tag::Alarm, Tag::Maintenance]),
        when: &[Kind::Timeout],
        why: "타임아웃은 Unlocked 자동 재잠금에만 쓰인다",
    },
    Ignore {
        from: Source::AnyExcept(&[Tag::Alarm]),
        when: &[Kind::Reset],
        why: "Reset은 경보 해제 용도이므로 Alarm에서만 의미 있다",
    },
    Ignore {
        from: Source::These(&[Tag::Unlocked, Tag::Alarm, Tag::Maintenance]),
        when: &[Kind::EnterCode],
        why: "코드 입력은 잠금 해제 목적이므로 Locked에서만 받는다",
    },
    Ignore {
        from: Source::These(&[Tag::Unlocked, Tag::Alarm]),
        when: &[Kind::MaintenanceToggle],
        why: "점검 모드 전환은 Locked/Maintenance 사이에서만 일어난다",
    },
];

fn main() {
    let mut world = Env {
        correct_code: 1234,
        attempts: 0,
        max_attempts: 2,
    };
    let mut m = Machine::new(
        Tag::Locked,
        vec![
            Box::new(Locked),
            Box::new(Unlocked),
            Box::new(Alarm),
            Box::new(Maintenance),
        ],
        EDGES,
        IGNORES,
    );

    let steps: &[(&str, Event)] = &[
        ("틀린 코드 1회", Event::EnterCode(9999)),
        ("틀린 코드 2회", Event::EnterCode(8888)),
        ("틀린 코드 3회 -> 경보", Event::EnterCode(7777)),
        ("관리자 리셋", Event::Reset),
        ("맞는 코드", Event::EnterCode(1234)),
        ("타임아웃 -> 재잠금", Event::Timeout),
        ("점검 모드 진입", Event::MaintenanceToggle),
        ("점검 모드 해제", Event::MaintenanceToggle),
    ];

    println!("state = {:?}", m.tag());
    for (desc, ev) in steps {
        println!("dispatch: {desc} ({ev:?})");
        if let Some(taken) = m.dispatch(ev, &mut world) {
            println!("  edge = {}, state = {:?}", taken.edge, m.tag());
        } else {
            println!("  (무시됨)");
        }
    }

    let diagram = render::to_mermaid::<Door>(Tag::Locked, EDGES);
    let md = format!("# Door lock FSM\n\n```mermaid\n{diagram}```\n");
    std::fs::create_dir_all("example").expect("failed to create example dir");
    std::fs::write("example/door_lock.md", &md).expect("failed to write example/door_lock.md");
    println!("\nmermaid diagram written to example/door_lock.md");

    let coverage = render::coverage::<Door>(Tag::Locked, EDGES, IGNORES);
    println!("coverage clean: {}", coverage.is_clean());
}
