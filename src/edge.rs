//! One row of the transition table.

use super::{Domain, Expr};

/// Which states an edge departs from. This is a set of states, not a guard.
pub enum Source<D: Domain> {
    These(&'static [D::Tag]),
    /// Every state except the listed ones. Gives the DRY benefit of hierarchical
    /// state machines without the hierarchy.
    AnyExcept(&'static [D::Tag]),
    Any,
}

impl<D: Domain> Source<D> {
    pub fn matches(&self, tag: D::Tag) -> bool {
        match self {
            Self::These(list) => list.contains(&tag),
            Self::AnyExcept(list) => !list.contains(&tag),
            Self::Any => true,
        }
    }

    /// The concrete states this matches, for diagrams and coverage. Wildcards are
    /// expanded here.
    pub fn expand(&self) -> Vec<D::Tag> {
        D::all_tags()
            .iter()
            .copied()
            .filter(|t| self.matches(*t))
            .collect()
    }
}

/// The target of a transition.
pub enum Goto<D: Domain> {
    To(D::Tag),
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
pub struct Edge<D: Domain> {
    /// Stable identifier, for requirement tracing and golden diffs. It must
    /// survive reordering of the table.
    pub id: &'static str,
    pub from: Source<D>,
    pub when: D::EventKind,
    pub check: &'static Expr<D>,
    pub unknown: OnUnknown,
    /// Actions run only by this transition, in declaration order.
    pub run: &'static [D::Action],
    pub goto: Goto<D>,
}

/// A `(state, event kind)` combination that is deliberately not handled.
///
/// Declaring these is what lets coverage checking tell a gap apart from an
/// intentional omission, so `why` is required.
pub struct Ignore<D: Domain> {
    pub from: Source<D>,
    pub when: &'static [D::EventKind],
    pub why: &'static str,
}

impl<D: Domain> Ignore<D> {
    pub fn matches(&self, tag: D::Tag, kind: D::EventKind) -> bool {
        self.from.matches(tag) && self.when.contains(&kind)
    }
}
