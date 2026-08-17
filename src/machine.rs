//! The state machine layer: a transition table and the executor that runs it.

mod cond;
mod edge;
mod node;
mod state;

pub use cond::Cond;
pub use edge::{Edge, Goto, Ignore, OnUnknown, Source};
pub use node::{CondNode, Cx, Expr, Memo};
pub use state::State;

use crate::{Domain, HasKind, render};

/// The outcome of a transition, for tests and logs.
pub struct Taken<D: Domain> {
    /// The id of the edge that was selected.
    pub edge: &'static str,
    /// The actions that ran, in `exit_actions` → `run` → `entry_actions` order.
    pub actions: Vec<D::Action>,
}

// Derives would bound `D` itself; these bound only what is actually used.
impl<D: Domain> std::fmt::Debug for Taken<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Taken")
            .field("edge", &self.edge)
            .field("actions", &self.actions)
            .finish()
    }
}

impl<D: Domain> Clone for Taken<D> {
    fn clone(&self) -> Self {
        Self {
            edge: self.edge,
            actions: self.actions.clone(),
        }
    }
}

impl<D: Domain> PartialEq for Taken<D>
where
    D::Action: PartialEq,
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
pub struct Machine<D: Domain> {
    tag: D::Tag,
    states: &'static [State<D>],
    edges: &'static [Edge<D>],
    ignores: &'static [Ignore<D>],
}

impl<D: Domain> Machine<D> {
    /// Builds a machine and validates it.
    ///
    /// Panics if a state listed by [`Domain::all_tags`], or targeted by an edge, is
    /// missing from `states`. The second check matters when [`Domain::all_tags`] is
    /// narrowed to a subset, which takes the excluded tags out of the first.
    ///
    /// In debug builds it also panics when [`render::coverage`] reports a defect,
    /// so table gaps surface at construction rather than at runtime. Release builds
    /// skip that check; call [`render::coverage`] from a test to keep it enforced.
    pub fn new(
        initial: D::Tag,
        states: &'static [State<D>],
        edges: &'static [Edge<D>],
        ignores: &'static [Ignore<D>],
    ) -> Self {
        assert!(
            states.iter().any(|s| s.tag == initial),
            "initial tag {initial:?} is missing from the state table",
        );
        for &tag in D::all_tags() {
            assert!(
                states.iter().any(|s| s.tag == tag),
                "tag {tag:?} is listed in Domain::all_tags but not in the state table",
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
            let cov = render::coverage::<D>(initial, edges, ignores);
            assert!(cov.is_clean(), "[FSM] table has holes: {cov:?}");
        }

        Self {
            tag: initial,
            states,
            edges,
            ignores,
        }
    }

    pub fn tag(&self) -> D::Tag {
        self.tag
    }

    /// `new` checks the initial tag and every edge target, which are the only tags
    /// this is called with, so the miss arm cannot be reached.
    fn state_of(&self, tag: D::Tag) -> &'static State<D> {
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
    /// [`Domain::perform`] has no way to reach `Machine`, and `self` is mutably
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
    pub fn dispatch(&mut self, ev: &D::Event, world: &mut D::Env) -> Option<Taken<D>> {
        let kind = ev.kind();

        let Some(hit) = self.select(ev, world, kind) else {
            if !self.ignores.iter().any(|i| i.matches(self.tag, kind)) {
                log::warn!(
                    "[FSM] unhandled: {:?} x {ev:?} (no edge, no ignore)",
                    self.tag
                );
            }
            return None;
        };

        let edge = &self.edges[hit];
        let id = edge.id;
        let mut actions: Vec<D::Action> = Vec::new();

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
        log::debug!("[FSM] {id}: {ev:?} -> {:?} {actions:?}", self.tag);

        Some(Taken { edge: id, actions })
    }

    /// Returns the index of the first matching edge. Declaration order is
    /// priority.
    fn select(&self, ev: &D::Event, world: &D::Env, kind: D::EventKind) -> Option<usize> {
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
        to_run: &[D::Action],
        ev: &D::Event,
        world: &mut D::Env,
        done: &mut Vec<D::Action>,
    ) {
        for &a in to_run {
            D::perform(a, ev, world);
            done.push(a);
        }
    }
}
