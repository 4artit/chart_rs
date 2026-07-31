//! 실행기.

use std::collections::VecDeque;

use super::{Cond, Cx, Domain, Edge, Goto, Ignore, Memo, OnUnknown, StateNode, render};

/// 전이가 일어났을 때의 결과. 테스트·로그에서 쓴다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Taken {
    /// 채택된 엣지 id.
    pub edge: &'static str,
    /// 실행된 액션 이름들 (`on_exit` → `run` → `on_enter` 순).
    pub actions: Vec<String>,
}

pub struct Machine<D: Domain> {
    tag: D::Tag,
    states: Vec<Box<dyn StateNode<D>>>,
    edges: &'static [Edge<D>],
    ignores: &'static [Ignore<D>],
    queue: VecDeque<D::Event>,
}

impl<D: Domain> Machine<D> {
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

        // 표에 구멍·도달불가 상태가 있으면 Machine을 만드는 시점에 바로 패닉한다.
        // 릴리스 빌드에서는 비용이 사라진다 — coverage()가 커버 대상만큼 순회하므로
        // 무시하고 싶다면 별도로 render::coverage를 직접 호출해 검사하라.
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
            queue: VecDeque::new(),
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

    /// 이벤트 하나를 처리한다. 채택된 엣지가 없으면 `None`.
    ///
    /// 초기 상태의 `on_enter`는 실행되지 않는다 — 트리거 이벤트가 없기 때문이다.
    /// 초기 상태에 진입 액션이 필요하면 `Init` 이벤트를 정의해 dispatch 하라.
    ///
    /// 실행 순서: `on_exit(현재)` → `run` → 상태 전이 → `on_enter(목표)`.
    /// `Goto::Internal`이면 `run`만 실행된다.
    pub fn dispatch(&mut self, ev: &D::Event, world: &mut D::Env) -> Option<Taken> {
        let kind = D::kind(ev);

        let Some(hit) = self.select(ev, world, kind) else {
            if !self.ignores.iter().any(|i| i.matches(self.tag, kind)) {
                log::warn!(
                    "[FSM] unhandled: {:?} x {:?} (no edge, no ignore)",
                    self.tag,
                    kind
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
        log::debug!("[FSM] {id}: -> {:?} {names:?}", self.tag);
        self.perform_all(actions, world);

        Some(Taken {
            edge: id,
            actions: names,
        })
    }

    /// 큐가 빌 때까지 처리한다 (run-to-completion).
    ///
    /// 액션이 만든 이벤트는 [`Machine::post`]로 큐에 들어오며, 전이 하나가
    /// 완전히 끝난 뒤에만 처리된다. 재진입이 구조적으로 불가능하다.
    pub fn pump(&mut self, world: &mut D::Env) {
        while let Some(ev) = self.queue.pop_front() {
            self.dispatch(&ev, world);
        }
    }

    pub fn post(&mut self, ev: D::Event) {
        self.queue.push_back(ev);
    }

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

    fn perform_all(&self, actions: Vec<D::Action>, world: &mut D::Env) {
        let idx = self.index_of(self.tag);
        for a in actions {
            let state: &dyn StateNode<D> = &*self.states[idx];
            D::perform(a, state, world);
        }
    }
}
