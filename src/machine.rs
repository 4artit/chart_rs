//! The executor.

use super::{Cond, Cx, Domain, Edge, Goto, HasKind, Ignore, Memo, OnUnknown, StateNode, render};

/// The outcome of a transition, for tests and logs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Taken {
    /// The id of the edge that was selected.
    pub edge: &'static str,
    /// Names of the actions that ran, in `on_exit` → `run` → `on_enter` order.
    ///
    /// Names rather than values, so that `Taken` stays independent of
    /// `Domain::Action`.
    pub actions: Vec<String>,
}

/// The executor. Its only mutable state is the current tag and the state nodes.
///
/// There is no event queue. Re-entrancy is prevented by the borrow checker rather
/// than by queueing (see [`Machine::dispatch`]), and a caller-owned queue can
/// inspect each transition's [`Taken`].
pub struct Machine<D: Domain> {
    tag: D::Tag,
    states: Vec<Box<dyn StateNode<D>>>,
    edges: &'static [Edge<D>],
    ignores: &'static [Ignore<D>],
}

impl<D: Domain> Machine<D> {
    /// Builds a machine and validates it.
    ///
    /// Panics if a state listed by [`Domain::all_tags`] has no state node. In
    /// debug builds it also panics when [`render::coverage`] reports a defect, so
    /// table gaps surface at construction rather than at runtime. Release builds
    /// skip that check; call [`render::coverage`] from a test to keep it enforced.
    pub fn new(
        initial: D::Tag,
        states: Vec<Box<dyn StateNode<D>>>,
        edges: &'static [Edge<D>],
        ignores: &'static [Ignore<D>],
    ) -> Self {
        assert!(
            states.iter().any(|s| s.tag() == initial),
            "initial tag {initial:?} has no state node",
        );
        for &tag in D::all_tags() {
            assert!(
                states.iter().any(|s| s.tag() == tag),
                "tag {tag:?} is listed in Domain::all_tags but has no state node",
            );
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

    fn index_of(&self, tag: D::Tag) -> usize {
        self.states
            .iter()
            .position(|s| s.tag() == tag)
            .unwrap_or_else(|| panic!("no state node for {tag:?}"))
    }

    /// Handles one event. Returns `None` when no edge is selected.
    ///
    /// Order of effects: `on_exit(current)` → `run` → tag change →
    /// `on_enter(target)`. For [`Goto::Internal`] only `run` runs. All effects are
    /// collected first and carried out afterwards, so [`Domain::perform`] always
    /// receives the state node of the *target* state.
    ///
    /// The initial state's `on_enter` never runs, as no event triggered it. Define
    /// an `Init` event and dispatch it if the initial state needs entry effects.
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
    pub fn dispatch(&mut self, ev: &D::Event, world: &mut D::Env) -> Option<Taken> {
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

        if target.is_some() {
            let cur = self.index_of(self.tag);
            self.states[cur].on_exit(world, &mut actions);
        }

        actions.extend_from_slice(self.edges[hit].run);

        if let Some(next) = target {
            self.tag = next;
            let ni = self.index_of(next);
            self.states[ni].on_enter(ev, world, &mut actions);
        }

        let names = actions.iter().map(|a| format!("{a:?}")).collect();
        log::debug!("[FSM] {id}: {ev:?} -> {:?} {names:?}", self.tag);
        self.perform_all(actions, ev, world);

        Some(Taken {
            edge: id,
            actions: names,
        })
    }

    /// Returns the index of the first matching edge. Declaration order is
    /// priority.
    fn select(&self, ev: &D::Event, world: &D::Env, kind: D::EventKind) -> Option<usize> {
        let memo = Memo::new();
        let state: &dyn StateNode<D> = &*self.states[self.index_of(self.tag)];
        let cx = Cx::new(ev, world, state, &memo);

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

    fn perform_all(&self, actions: Vec<D::Action>, ev: &D::Event, world: &mut D::Env) {
        let idx = self.index_of(self.tag);
        for a in actions {
            let state: &dyn StateNode<D> = &*self.states[idx];
            D::perform(a, ev, state, world);
        }
    }
}
