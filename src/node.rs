//! 조건 노드와 조건식 트리.

use std::any::Any;
use std::cell::RefCell;

use super::{Cond, Domain, StateNode};

/// 전이 조건 하나.
///
/// **내부 상태를 가질 수 없다.** `eval`이 `&self`를 받고 노드가 `&'static`으로
/// 보관되므로, 필요한 모든 입력은 [`Cx`]로 주입된다. 이 규약 덕분에
/// [`Memo`] 캐시가 안전하고 노드가 순수 함수로 테스트된다.
pub trait CondNode<D: Domain>: Sync + Any {
    /// 다이어그램·로그에 찍히는 이름. **머신 안에서 유일해야 한다** (Memo 키).
    ///
    /// 유일성은 [`super::render::coverage`]가 검사한다.
    fn name(&self) -> &'static str;

    /// 조건을 판정한다. 세상을 변경해서는 안 된다.
    fn eval(&self, cx: &Cx<'_, D>) -> Cond;
}

/// 조건 노드에 주입되는 입력.
pub struct Cx<'a, D: Domain> {
    /// 이벤트 payload.
    pub event: &'a D::Event,
    /// 바깥 세상 — 읽기 전용.
    pub world: &'a D::Env,
    /// 현재 상태 노드. **태그로 분기하지 말고 변수만 읽어라.**
    pub state: &'a dyn StateNode<D>,
    memo: &'a Memo,
}

impl<'a, D: Domain> Cx<'a, D> {
    pub fn new(
        event: &'a D::Event,
        world: &'a D::Env,
        state: &'a dyn StateNode<D>,
        memo: &'a Memo,
    ) -> Self {
        Self {
            event,
            world,
            state,
            memo,
        }
    }

    /// 현재 상태 노드를 구체 타입으로 본다. 다른 상태면 `None`.
    ///
    /// 엣지의 `from`이 이미 상태를 확정했으므로 호출부에서는 안전하다.
    pub fn state_as<S: StateNode<D>>(&self) -> Option<&S> {
        self.state.as_any().downcast_ref::<S>()
    }
}

/// dispatch 1회 동안만 유효한 조건 평가 캐시.
///
/// 같은 `(상태, 이벤트)`에 엣지가 여러 개면 동일 노드가 여러 번 평가된다.
/// 캐시가 없으면 **한 번의 이벤트 처리 안에서 세상이 두 개가 될 수 있다.**
#[derive(Default)]
pub struct Memo {
    cache: RefCell<Vec<(&'static str, Cond)>>,
}

impl Memo {
    pub fn new() -> Self {
        Self::default()
    }

    fn lookup(&self, name: &'static str) -> Option<Cond> {
        self.cache
            .borrow()
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, c)| *c)
    }

    fn store(&self, name: &'static str, cond: Cond) {
        self.cache.borrow_mut().push((name, cond));
    }
}

/// 조건식 트리. `check!` 매크로가 이 형태로 전개한다.
pub enum Expr<D: Domain> {
    /// 조건 없음 — 항상 참.
    Always,
    Node(&'static dyn CondNode<D>),
    And(&'static Expr<D>, &'static Expr<D>),
    Or(&'static Expr<D>, &'static Expr<D>),
    Not(&'static Expr<D>),
}

impl<D: Domain> Expr<D> {
    /// 트리를 평가한다. `And`/`Or`는 **단축평가**하므로 불필요한 api 호출이 없다.
    pub fn eval(&self, cx: &Cx<'_, D>) -> Cond {
        match self {
            Self::Always => Cond::True,
            Self::Node(n) => {
                let name = n.name();
                if let Some(cached) = cx.memo.lookup(name) {
                    return cached;
                }
                let cond = n.eval(cx);
                cx.memo.store(name, cond);
                cond
            }
            Self::And(l, r) => match l.eval(cx) {
                Cond::False => Cond::False,
                left => left.and(r.eval(cx)),
            },
            Self::Or(l, r) => match l.eval(cx) {
                Cond::True => Cond::True,
                left => left.or(r.eval(cx)),
            },
            Self::Not(x) => x.eval(cx).not(),
        }
    }

    /// 다이어그램용 문자열. `A && !B` 형태로 되돌린다.
    pub fn render(&self) -> String {
        match self {
            Self::Always => String::new(),
            Self::Node(n) => n.name().to_owned(),
            Self::And(l, r) => format!("{} && {}", l.render(), r.render()),
            Self::Or(l, r) => format!("({} || {})", l.render(), r.render()),
            Self::Not(x) => format!("!{}", x.render()),
        }
    }

    /// 이 식이 참조하는 모든 노드의 `(이름, 타입)`. 이름 유일성 검사에 쓴다.
    pub fn node_ids(&self, out: &mut Vec<(&'static str, std::any::TypeId)>) {
        match self {
            Self::Always => {}
            Self::Node(n) => out.push((n.name(), (*n).type_id())),
            Self::And(l, r) | Self::Or(l, r) => {
                l.node_ids(out);
                r.node_ids(out);
            }
            Self::Not(x) => x.node_ids(out),
        }
    }
}

/// 조건 노드를 선언한다. 유닛 구조체 + `CondNode` 구현을 만든다.
///
/// ```ignore
/// cond_node!(RearCam, GearIsReverse, |cx| match cx.event {
///     Event::GearChanged(g) => Cond::from(*g == Gear::Reverse),
///     _ => Cond::False,
/// });
/// ```
#[macro_export]
macro_rules! cond_node {
    ($dom:ty, $name:ident, |$cx:ident| $body:expr) => {
        #[derive(Copy, Clone)]
        pub struct $name;

        impl $crate::CondNode<$dom> for $name {
            fn name(&self) -> &'static str {
                stringify!($name)
            }
            fn eval(&self, $cx: &$crate::Cx<'_, $dom>) -> $crate::Cond {
                $body
            }
        }
    };
}

/// 조건식을 만든다. `&&` 체인과 앞선 `!`를 지원한다.
///
/// `||` 조합까지 지원하면 매크로 규칙이 훨씬 복잡해지는 데 비해 실전에서
/// 쓰이는 빈도는 낮아 의도적으로 뺐다. 필요하면 [`Expr::Or`]를 직접 쓰거나
/// proc-macro 버전으로 확장한다.
#[macro_export]
macro_rules! check {
    () => { &$crate::Expr::Always };
    (! $n:ident && $($rest:tt)*) => {
        &$crate::Expr::And(
            &$crate::Expr::Not(&$crate::Expr::Node(&$n)),
            $crate::check!($($rest)*),
        )
    };
    ($n:ident && $($rest:tt)*) => {
        &$crate::Expr::And(
            &$crate::Expr::Node(&$n),
            $crate::check!($($rest)*),
        )
    };
    (! $n:ident) => { &$crate::Expr::Not(&$crate::Expr::Node(&$n)) };
    ($n:ident) => { &$crate::Expr::Node(&$n) };
}
