//! 폴드 — 상태 있음.
//!
//! "접는 중"과 "펴는 중"이 관찰 가능한 진행 상태다. 같은 `PowerChanged`가 상태에
//! 따라 다른 결과를 내므로 `machine` 층을 쓴다.
//!
//! # 머신 스펙
//!
//! [`MachineSpec`]이 "어느 도메인의, 어떤 상태들인가"만 말한다. 어휘도 guard도
//! `perform`도 [`crate::Mirrors`]와 그대로 공유하므로, 상태 기계를 하나 더 추가할
//! 때도 이 두 줄만 더 쓰면 된다.

use chart::machine::{Cond, Edge, Goto, Ignore, Machine, OnUnknown, Source, State};
use chart::render::Coverage;
use chart::{MachineSpec, render};

use crate::{Action, Event, Kind, Mirrors, World};

chart::tags! {
    pub enum FoldTag {
        Unfolded,
        Folding,
        Folded,
        Unfolding,
    }
}

pub struct FoldSm;

impl MachineSpec for FoldSm {
    type Domain = Mirrors;
    type Tag = FoldTag;
}

// ─────────────────────────────────────────── guards
// 도메인에 붙으므로 다른 머신이 그대로 재사용할 수 있다.

chart::cond_node!(Mirrors, PowerOff, |cx| Cond::from(!cx.world.power_on));
chart::cond_node!(Mirrors, PowerOn, |cx| Cond::from(cx.world.power_on));
chart::cond_node!(Mirrors, SpeedAllowsFold, |cx| Cond::from(
    cx.world.speed < 15.0
));
chart::cond_node!(Mirrors, SpeedForcesUnfold, |cx| Cond::from(
    cx.world.speed >= 40.0
));
chart::cond_node!(Mirrors, AtFolded, |cx| Cond::from(
    cx.world.fold_position <= 0.01
));
chart::cond_node!(Mirrors, AtUnfolded, |cx| Cond::from(
    cx.world.fold_position >= 0.99
));

// ─────────────────────────────────────────── 표

/// 모터 명령은 진입 액션이다. 어느 경로로 들어오든 한 번만 나간다.
static STATES: &[State<FoldSm>] = &[
    State {
        tag: FoldTag::Unfolded,
        entry: &[],
        exit: &[],
    },
    State {
        tag: FoldTag::Folding,
        entry: &[Action::Fold],
        exit: &[],
    },
    State {
        tag: FoldTag::Folded,
        entry: &[],
        exit: &[],
    },
    State {
        tag: FoldTag::Unfolding,
        entry: &[Action::Unfold],
        exit: &[],
    },
];

static EDGES: &[Edge<FoldSm>] = &[
    Edge {
        id: "FOLD_START",
        from: Source::These(&[FoldTag::Unfolded]),
        when: Kind::PowerChanged,
        check: chart::check!(PowerOff && SpeedAllowsFold),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(FoldTag::Folding),
    },
    Edge {
        id: "FOLD_DONE",
        from: Source::These(&[FoldTag::Folding]),
        when: Kind::FoldPositionChanged,
        check: chart::check!(AtFolded),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(FoldTag::Folded),
    },
    Edge {
        id: "UNFOLD_ON_POWER",
        from: Source::These(&[FoldTag::Folded]),
        when: Kind::PowerChanged,
        check: chart::check!(PowerOn),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(FoldTag::Unfolding),
    },
    Edge {
        id: "UNFOLD_ON_SPEED",
        from: Source::These(&[FoldTag::Folded]),
        when: Kind::SpeedChanged,
        check: chart::check!(SpeedForcesUnfold),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(FoldTag::Unfolding),
    },
    Edge {
        id: "UNFOLD_DONE",
        from: Source::These(&[FoldTag::Unfolding]),
        when: Kind::FoldPositionChanged,
        check: chart::check!(AtUnfolded),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(FoldTag::Unfolded),
    },
];

/// 다루지 않는 조합은 이유와 함께 선언한다. 이게 있어야 `coverage`가 "빠뜨림"과
/// "의도"를 구분한다.
static IGNORES: &[Ignore<FoldSm>] = &[
    Ignore {
        from: Source::Any,
        when: &[Kind::DefogChanged, Kind::GearChanged, Kind::UserChanged],
        why: "난방·방현·사용자 전환은 폴드와 무관하다",
    },
    Ignore {
        from: Source::These(&[FoldTag::Folding, FoldTag::Unfolding]),
        when: &[Kind::PowerChanged],
        why: "이동 중에는 전원 변화로 방향을 뒤집지 않는다",
    },
    Ignore {
        from: Source::These(&[FoldTag::Unfolded, FoldTag::Folding, FoldTag::Unfolding]),
        when: &[Kind::SpeedChanged],
        why: "자동 펴기는 접힌 상태에서만 의미가 있다",
    },
    Ignore {
        from: Source::These(&[FoldTag::Unfolded, FoldTag::Folded]),
        when: &[Kind::FoldPositionChanged],
        why: "정지 상태에서의 위치 보고는 확인할 목표가 없다",
    },
];

// ─────────────────────────────────────────── 기능

pub struct Fold(Machine<FoldSm>);

impl Default for Fold {
    fn default() -> Self {
        Self(Machine::new(FoldTag::Unfolded, STATES, EDGES, IGNORES))
    }
}

impl Fold {
    pub fn dispatch(
        &mut self,
        ev: &Event,
        world: &mut World,
    ) -> Option<chart::machine::Taken<FoldSm>> {
        self.0.dispatch(ev, world)
    }
}

/// 문서 생성기. 표에서만 나오므로 머신 인스턴스가 필요 없다.
pub fn diagram() -> String {
    render::to_mermaid::<FoldSm>(FoldTag::Unfolded, EDGES, STATES)
}

/// 이 머신이 다루는 이벤트 종류. 컨트롤러 전체 진단에 넘긴다.
pub fn handled_kinds() -> Vec<Kind> {
    render::handled_kinds::<FoldSm>(EDGES)
}

pub fn coverage() -> Coverage {
    render::coverage::<FoldSm>(FoldTag::Unfolded, EDGES, IGNORES)
}
