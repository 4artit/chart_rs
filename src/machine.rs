//! The state machine layer: a transition table and the executor that runs it.

mod cond;
mod edge;
mod node;
mod state;

pub use cond::Cond;
pub use edge::{Edge, Goto, Ignore, OnUnknown, Source};
pub use node::{CondNode, Cx, Expr, Memo};
pub use state::State;

use crate::{ActionOf, Domain, EnvOf, EventOf, HasKind, KindOf, MachineSpec, render};

/// The outcome of one [`Machine::dispatch`] call, for tests and logs.
pub struct Taken<M: MachineSpec> {
    /// The id of the edge that was selected.
    pub edge: &'static str,
    /// The actions that ran, in `exit` → `run` → `entry` order.
    pub actions: Vec<ActionOf<M>>,
}

// Derives would bound `D` itself; these bound only what is actually used.
impl<M: MachineSpec> std::fmt::Debug for Taken<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Taken")
            .field("edge", &self.edge)
            .field("actions", &self.actions)
            .finish()
    }
}

impl<M: MachineSpec> Clone for Taken<M> {
    fn clone(&self) -> Self {
        Self {
            edge: self.edge,
            actions: self.actions.clone(),
        }
    }
}

impl<M: MachineSpec> PartialEq for Taken<M>
where
    ActionOf<M>: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.edge == other.edge && self.actions == other.actions
    }
}

/// Runs a transition table: holds the current state and dispatches events
/// against it.
pub struct Machine<M: MachineSpec> {
    tag: M::Tag,
    states: &'static [State<M>],
    edges: &'static [Edge<M>],
    ignores: &'static [Ignore<M>],
}

impl<M: MachineSpec> Machine<M> {
    /// Builds a machine and validates its tables.
    ///
    /// - `initial`: the starting state tag.
    /// - `states`, `edges`, `ignores`: the transition table, as static slices.
    ///
    /// Returns the constructed machine.
    ///
    /// # Panics
    ///
    /// Panics if `initial`, or any tag [`MachineSpec::all_tags`] lists, or any
    /// edge target, is missing from `states`. In debug builds, also panics if
    /// [`render::coverage`] reports a defect (release builds skip that check;
    /// call [`render::coverage`] from a test to keep it enforced there).
    pub fn new(
        initial: M::Tag,
        states: &'static [State<M>],
        edges: &'static [Edge<M>],
        ignores: &'static [Ignore<M>],
    ) -> Self {
        assert!(
            states.iter().any(|s| s.tag == initial),
            "initial tag {initial:?} is missing from the state table",
        );
        for &tag in M::all_tags() {
            assert!(
                states.iter().any(|s| s.tag == tag),
                "tag {tag:?} is listed in MachineSpec::all_tags but not in the state table",
            );
        }
        for e in edges {
            if let Goto::To(next) = e.goto {
                assert!(
                    states.iter().any(|s| s.tag == next),
                    "edge {} goes to {next:?}, which is missing from the state table",
                    e.id,
                );
            }
        }

        #[cfg(debug_assertions)]
        {
            let cov = render::coverage::<M>(initial, edges, ignores);
            assert!(cov.is_clean(), "[chart] table has holes: {cov:?}");
        }

        Self {
            tag: initial,
            states,
            edges,
            ignores,
        }
    }

    /// Returns the current state tag.
    pub fn tag(&self) -> M::Tag {
        self.tag
    }

    /// Looks up the state table entry for `tag`. `new` guarantees every tag
    /// this is called with is present, so the miss arm is unreachable.
    fn state_of(&self, tag: M::Tag) -> &'static State<M> {
        self.states
            .iter()
            .find(|s| s.tag == tag)
            .unwrap_or_else(|| unreachable!("no state table entry for {tag:?}"))
    }

    /// Matches `ev` against the table from the current state and runs the
    /// selected transition.
    ///
    /// - `ev`: the event to handle.
    /// - `world`: the outside world, mutated by whatever actions run.
    ///
    /// Returns the [`Taken`] transition, or `None` if no edge matched (a
    /// warning is logged unless the combination is covered by an [`Ignore`]).
    ///
    /// Effects run in this order: the current state's [`State::exit`] → the
    /// tag changes → `run` → the target state's [`State::entry`]. For
    /// [`Goto::Internal`] only `run` executes. The initial state's entry
    /// actions never run on construction; dispatch an explicit init event if
    /// they're needed.
    ///
    /// Not re-entrant: `self` is mutably borrowed for the call, so a nested
    /// dispatch won't compile. Queue follow-up events in the caller instead —
    /// see [`Taken`].
    pub fn dispatch(&mut self, ev: &EventOf<M>, world: &mut EnvOf<M>) -> Option<Taken<M>> {
        let kind = ev.kind();

        let Some(hit) = self.select(ev, world, kind) else {
            if !self.ignores.iter().any(|i| i.matches(self.tag, kind)) {
                log::warn!(
                    "[chart] unhandled: {:?} x {ev:?} (no edge, no ignore)",
                    self.tag
                );
            }
            return None;
        };

        let edge = &self.edges[hit];
        let id = edge.id;
        let mut actions: Vec<ActionOf<M>> = Vec::new();

        let target = match edge.goto {
            Goto::To(next) => Some(next),
            Goto::Internal => None,
        };

        if let Some(next) = target {
            Self::perform_all(self.state_of(self.tag).exit, ev, world, &mut actions);
            self.tag = next;
        }

        Self::perform_all(self.edges[hit].run, ev, world, &mut actions);

        if let Some(next) = target {
            Self::perform_all(self.state_of(next).entry, ev, world, &mut actions);
        }

        // `log::debug!` evaluates its arguments only when the level is enabled,
        // so this formats nothing in a release build with logging off.
        log::debug!("[chart] {id}: {ev:?} -> {:?} {actions:?}", self.tag);

        Some(Taken { edge: id, actions })
    }

    /// Index of the first edge matching `kind` from the current state.
    /// Declaration order is priority.
    fn select(&self, ev: &EventOf<M>, world: &EnvOf<M>, kind: KindOf<M>) -> Option<usize> {
        let memo = Memo::new();
        let cx = Cx::new(ev, world, &memo);

        self.edges.iter().position(|e| {
            e.when == kind
                && e.from.matches(self.tag)
                && match e.check.eval(&cx) {
                    Cond::True => true,
                    Cond::False => false,
                    Cond::Unknown => e.unknown == OnUnknown::Allow,
                }
        })
    }

    /// Runs each action in `to_run` in order, appending it to `done`.
    fn perform_all(
        to_run: &[ActionOf<M>],
        ev: &EventOf<M>,
        world: &mut EnvOf<M>,
        done: &mut Vec<ActionOf<M>>,
    ) {
        for &a in to_run {
            <M::Domain as Domain>::perform(a, ev, world);
            done.push(a);
        }
    }
}
