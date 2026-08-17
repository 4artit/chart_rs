//! The executor.

use super::{Cond, Cx, Domain, Edge, Goto, HasKind, Ignore, Memo, OnUnknown, StateNode, render};

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

    /// The state nodes, for [`render::to_mermaid`].
    pub fn states(&self) -> &[Box<dyn StateNode<D>>] {
        &self.states
    }

    fn index_of(&self, tag: D::Tag) -> usize {
        self.states
            .iter()
            .position(|s| s.tag() == tag)
            .unwrap_or_else(|| panic!("no state node for {tag:?}"))
    }

    /// Handles one event. Returns `None` when no edge is selected.
    ///
    /// Order of effects: [`StateNode::exit_actions`] → `on_exit` → tag change →
    /// `run` → `on_enter` → [`StateNode::entry_actions`]. For [`Goto::Internal`]
    /// only `run` runs. Each action is carried out while the machine is in the
    /// state that owns it, so [`Domain::perform`] receives the state being left for
    /// an exit action and the target state for the rest.
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
            let cur = self.index_of(self.tag);
            self.perform_all(self.states[cur].exit_actions(), ev, world, &mut actions);
            self.states[cur].on_exit(world);
            self.tag = next;
        }

        self.perform_all(self.edges[hit].run, ev, world, &mut actions);

        if let Some(next) = target {
            let ni = self.index_of(next);
            self.states[ni].on_enter(ev, world);
            self.perform_all(self.states[ni].entry_actions(), ev, world, &mut actions);
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

    /// Carries out `to_run` against the state the machine is in *now*, appending
    /// each action to `done` as it goes.
    fn perform_all(
        &self,
        to_run: &[D::Action],
        ev: &D::Event,
        world: &mut D::Env,
        done: &mut Vec<D::Action>,
    ) {
        let idx = self.index_of(self.tag);
        let state: &dyn StateNode<D> = &*self.states[idx];

        for &a in to_run {
            D::perform(a, ev, state, world);
            done.push(a);
        }
    }
}
