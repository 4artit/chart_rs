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

/// The outcome of a transition, for tests and logs.
pub struct Taken<M: MachineSpec> {
    /// The id of the edge that was selected.
    pub edge: &'static str,
    /// The actions that ran, in `exit_actions` → `run` → `entry_actions` order.
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

/// The executor. Its only mutable state is the current tag; everything else is
/// the static tables it was built from.
///
/// There is no event queue. Re-entrancy is prevented by the borrow checker rather
/// than by queueing (see [`Machine::dispatch`]), and a caller-owned queue can
/// inspect each transition's [`Taken`].
pub struct Machine<M: MachineSpec> {
    tag: M::Tag,
    states: &'static [State<M>],
    edges: &'static [Edge<M>],
    ignores: &'static [Ignore<M>],
}

impl<M: MachineSpec> Machine<M> {
    /// Builds a machine and validates it.
    ///
    /// Panics if a state listed by [`MachineSpec::all_tags`], or targeted by an edge, is
    /// missing from `states`. The second check matters when [`MachineSpec::all_tags`] is
    /// narrowed to a subset, which takes the excluded tags out of the first.
    ///
    /// In debug builds it also panics when [`render::coverage`] reports a defect,
    /// so table gaps surface at construction rather than at runtime. Release builds
    /// skip that check; call [`render::coverage`] from a test to keep it enforced.
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

    pub fn tag(&self) -> M::Tag {
        self.tag
    }

    /// `new` checks the initial tag and every edge target, which are the only tags
    /// this is called with, so the miss arm cannot be reached.
    fn state_of(&self, tag: M::Tag) -> &'static State<M> {
        self.states
            .iter()
            .find(|s| s.tag == tag)
            .unwrap_or_else(|| unreachable!("no state table entry for {tag:?}"))
    }

    /// Handles one event. Returns `None` when no edge is selected.
    ///
    /// Order of effects: [`State::exit`] of the current state → tag change → `run`
    /// → [`State::entry`] of the target. For [`Goto::Internal`] only `run` runs.
    /// Each action is carried out as it is reached, so an exit action sees the
    /// world before the transition's later effects.
    ///
    /// The initial state's entry effects never run, as no event triggered them.
    /// Define an `Init` event and dispatch it if the initial state needs them.
    ///
    /// # Re-entrancy
    ///
    /// [`crate::Domain::perform`] has no way to reach `Machine`, and `self` is mutably
    /// borrowed for the duration of this call, so a nested dispatch does not
    /// compile. Drive follow-up events from the caller:
    ///
    /// ```ignore
    /// let mut pending = VecDeque::from([first_event]);
    /// while let Some(ev) = pending.pop_front() {
    ///     if let Some(taken) = m.dispatch(&ev, &mut world) {
    ///         // decide follow-ups from taken.edge / taken.actions
    ///     }
    /// }
    /// ```
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

    /// Returns the index of the first matching edge. Declaration order is
    /// priority.
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

    /// Carries out `to_run`, appending each action to `done` as it goes.
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
