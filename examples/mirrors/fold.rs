//! Mirror folding. `PowerChanged` means different things while folding and
//! while folded, so this one is a state machine.
//!
//! The spec names [`crate::Mirrors`] as its domain, sharing that controller's
//! events, actions, guards and `perform`.

use chart::machine::{Cond, Edge, Goto, Ignore, Machine, OnUnknown, Source, State};
use chart::render::Coverage;
use chart::{MachineSpec, render};

use crate::{Event, Kind, Mirrors, StateAction, World};

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
// Declared against the domain, so a second machine could reuse them.

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

// ─────────────────────────────────────────── table

/// The motor command is an entry action: it goes out once per arrival, whichever
/// edge led there.
static STATES: &[State<FoldSm>] = &[
    State {
        tag: FoldTag::Unfolded,
        entry: &[],
        exit: &[],
    },
    State {
        tag: FoldTag::Folding,
        entry: &[StateAction::Fold],
        exit: &[],
    },
    State {
        tag: FoldTag::Folded,
        entry: &[],
        exit: &[],
    },
    State {
        tag: FoldTag::Unfolding,
        entry: &[StateAction::Unfold],
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

/// Combinations left alone, with the reason `coverage` needs to tell an omission
/// from a decision.
static IGNORES: &[Ignore<FoldSm>] = &[
    Ignore {
        from: Source::Any,
        when: &[Kind::DefogChanged, Kind::GearChanged, Kind::UserChanged],
        why: "heating, dimming and user switching do not affect folding",
    },
    Ignore {
        from: Source::These(&[FoldTag::Folding, FoldTag::Unfolding]),
        when: &[Kind::PowerChanged],
        why: "a power change must not reverse a motor that is already moving",
    },
    Ignore {
        from: Source::These(&[FoldTag::Unfolded, FoldTag::Folding, FoldTag::Unfolding]),
        when: &[Kind::SpeedChanged],
        why: "the automatic unfold only applies once folded",
    },
    Ignore {
        from: Source::These(&[FoldTag::Unfolded, FoldTag::Folded]),
        when: &[Kind::FoldPositionChanged],
        why: "position reports while stopped have no target to check against",
    },
];

// ─────────────────────────────────────────── feature

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

/// Drawn from the tables alone; no machine instance needed.
pub fn diagram() -> String {
    render::to_mermaid::<FoldSm>(FoldTag::Unfolded, EDGES, STATES)
}

/// The kinds this machine acts on, for the controller-wide check.
pub fn handled_kinds() -> Vec<Kind> {
    render::handled_kinds::<FoldSm>(EDGES)
}

pub fn coverage() -> Coverage {
    render::coverage::<FoldSm>(FoldTag::Unfolded, EDGES, IGNORES)
}
