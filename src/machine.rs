//! The state machine layer: a transition table and the executor that runs it.

mod cond;
mod edge;
mod node;
mod state;

pub use cond::Cond;
pub use edge::{Edge, Goto, Ignore, OnUnknown, Source};
pub use node::{CondNode, Cx, Expr, Memo};
pub use state::State;

use crate::{
    ActionOf, Domain, EnvOf, EventOf, HasKind, KindOf, MachineSpec, StateActionOf, render,
};

/// The outcome of one [`Machine::dispatch`] call, for tests and logs.
///
/// The three lists are borrowed from the tables and each ran in full. Field
/// order is execution order.
pub struct Taken<M: MachineSpec> {
    /// The id of the edge that was selected.
    pub edge: &'static str,
    /// The left state's exit actions. Empty for [`Goto::Internal`].
    pub exit: &'static [StateActionOf<M>],
    /// The selected edge's own actions.
    pub run: &'static [ActionOf<M>],
    /// The entered state's entry actions. Empty for [`Goto::Internal`].
    pub entry: &'static [StateActionOf<M>],
}

// Derives would bound `D` itself; these bound only what is actually used.
impl<M: MachineSpec> std::fmt::Debug for Taken<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Taken")
            .field("edge", &self.edge)
            .field("exit", &self.exit)
            .field("run", &self.run)
            .field("entry", &self.entry)
            .finish()
    }
}

impl<M: MachineSpec> Clone for Taken<M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M: MachineSpec> Copy for Taken<M> {}

impl<M: MachineSpec> PartialEq for Taken<M>
where
    ActionOf<M>: PartialEq,
    StateActionOf<M>: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.edge == other.edge
            && self.exit == other.exit
            && self.run == other.run
            && self.entry == other.entry
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
    /// - `initial`: the state the world is already in, which the caller keeps
    ///   consistent with `Env`. A machine resumes rather than starts, so its
    ///   [`State::entry`] does not run and this call does not touch `Env`.
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
    /// [`Goto::Internal`] only `run` executes. Entry and exit go through
    /// [`Domain::perform_state`], which is not given the event; only `run`
    /// reaches [`Domain::perform`].
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
        let run = edge.run;

        let target = match edge.goto {
            Goto::To(next) => Some(next),
            Goto::Internal => None,
        };

        // Empty for an internal transition, which stays in its state.
        let mut exit: &'static [StateActionOf<M>] = &[];
        let mut entry: &'static [StateActionOf<M>] = &[];

        if let Some(next) = target {
            exit = self.state_of(self.tag).exit;
            entry = self.state_of(next).entry;
            self.tag = next;
        }

        Self::perform_state_all(exit, world);
        Self::perform_all(run, ev, world);
        Self::perform_state_all(entry, world);

        // `log::debug!` evaluates its arguments only when the level is enabled,
        // so this formats nothing in a release build with logging off.
        log::debug!(
            "[chart] {id}: {ev:?} -> {:?} {exit:?} {run:?} {entry:?}",
            self.tag
        );

        Some(Taken {
            edge: id,
            exit,
            run,
            entry,
        })
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

    /// Runs each of an edge's actions in order.
    fn perform_all(to_run: &[ActionOf<M>], ev: &EventOf<M>, world: &mut EnvOf<M>) {
        for &a in to_run {
            <M::Domain as Domain>::perform(a, ev, world);
        }
    }

    /// Runs each of a state's entry or exit actions in order.
    fn perform_state_all(to_run: &[StateActionOf<M>], world: &mut EnvOf<M>) {
        for &a in to_run {
            <M::Domain as Domain>::perform_state(a, world);
        }
    }
}
