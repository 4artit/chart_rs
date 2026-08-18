//! One row of the transition table.

use crate::{ActionOf, KindOf, MachineSpec};

use super::Expr;

/// The set of states an edge departs from — a state list, not a guard
/// condition.
pub enum Source<M: MachineSpec> {
    These(&'static [M::Tag]),
    /// Every state except the listed ones.
    AnyExcept(&'static [M::Tag]),
    Any,
}

impl<M: MachineSpec> Source<M> {
    /// Reports whether `tag` is in this source's state set.
    pub fn matches(&self, tag: M::Tag) -> bool {
        match self {
            Self::These(list) => list.contains(&tag),
            Self::AnyExcept(list) => !list.contains(&tag),
            Self::Any => true,
        }
    }

    /// Expands this source into its concrete list of states, for diagrams and
    /// coverage checking.
    pub fn expand(&self) -> Vec<M::Tag> {
        M::all_tags()
            .iter()
            .copied()
            .filter(|t| self.matches(*t))
            .collect()
    }
}

/// The target of a transition.
pub enum Goto<M: MachineSpec> {
    To(M::Tag),
    /// Stay in the current state. `on_exit` and `on_enter` do **not** run.
    Internal,
}

/// What to do when a guard evaluates to [`super::Cond::Unknown`].
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum OnUnknown {
    /// Do not transition when undecidable (fail-closed).
    Deny,
    /// Transition when undecidable.
    Allow,
}

/// A single transition.
///
/// When several edges match the same `(state, event kind)`, **declaration order
/// is priority**.
pub struct Edge<M: MachineSpec> {
    /// Stable identifier, for requirement tracing and golden diffs. It must
    /// survive reordering of the table.
    pub id: &'static str,
    pub from: Source<M>,
    pub when: KindOf<M>,
    pub check: &'static Expr<M::Domain>,
    pub unknown: OnUnknown,
    /// Actions run only by this transition, in declaration order.
    pub run: &'static [ActionOf<M>],
    pub goto: Goto<M>,
}

/// A `(state, event kind)` combination that is deliberately not handled.
///
/// Declaring these lets coverage checking tell a gap apart from an
/// intentional omission, so `why` is required.
pub struct Ignore<M: MachineSpec> {
    pub from: Source<M>,
    pub when: &'static [KindOf<M>],
    /// Why this combination is intentionally unhandled.
    pub why: &'static str,
}

impl<M: MachineSpec> Ignore<M> {
    /// Reports whether this `Ignore` covers `(tag, kind)`.
    pub fn matches(&self, tag: M::Tag, kind: KindOf<M>) -> bool {
        self.from.matches(tag) && self.when.contains(&kind)
    }
}
