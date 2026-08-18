//! Guard nodes and the guard expression tree.

use std::any::Any;
use std::cell::RefCell;

use crate::Domain;

use super::Cond;

/// One transition guard.
///
/// Nodes are stateless: `eval` takes `&self`, and every input arrives
/// through [`Cx`]. That keeps a node testable as a pure function and makes
/// the [`Memo`] cache safe to share across edges.
pub trait CondNode<D: Domain>: Sync + Any {
    /// The name shown in diagrams and logs, and the [`Memo`] cache key.
    /// **Must be unique within a machine** — [`crate::render::coverage`]
    /// verifies this.
    fn name(&self) -> &'static str;

    /// Evaluates the guard against `cx`. Must not modify the world.
    ///
    /// Returns the guard's result.
    fn eval(&self, cx: &Cx<'_, D>) -> Cond;
}

/// The inputs handed to a guard node: the event, the world, and the
/// per-dispatch [`Memo`] cache.
pub struct Cx<'a, D: Domain> {
    /// The event, including payload.
    pub event: &'a D::Event,
    /// The outside world, read-only.
    pub world: &'a D::Env,
    memo: &'a Memo,
}

impl<'a, D: Domain> Cx<'a, D> {
    /// Builds a context from an event, a world reference, and a memo cache.
    pub fn new(event: &'a D::Event, world: &'a D::Env, memo: &'a Memo) -> Self {
        Self { event, world, memo }
    }
}

/// Guard evaluation cache, valid for one [`crate::machine::Machine::dispatch`]
/// call, so a node shared by several edges is evaluated only once per event.
#[derive(Default)]
pub struct Memo {
    cache: RefCell<Vec<(&'static str, Cond)>>,
}

impl Memo {
    /// Builds an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the cached result for `name`, if any.
    fn lookup(&self, name: &'static str) -> Option<Cond> {
        self.cache
            .borrow()
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, c)| *c)
    }

    /// Caches `cond` under `name`.
    fn store(&self, name: &'static str, cond: Cond) {
        self.cache.borrow_mut().push((name, cond));
    }
}

/// A guard expression tree. [`check!`] expands to this shape.
pub enum Expr<D: Domain> {
    /// No guard: always true.
    Always,
    Node(&'static dyn CondNode<D>),
    And(&'static Expr<D>, &'static Expr<D>),
    Or(&'static Expr<D>, &'static Expr<D>),
    Not(&'static Expr<D>),
}

impl<D: Domain> Expr<D> {
    /// Evaluates the tree against `cx`. `And`/`Or` short-circuit.
    ///
    /// Returns the combined guard result.
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

    /// Renders the expression for diagrams, e.g. `A && !B`. `Not` is
    /// parenthesised so `!(A && B)` doesn't read as `!A && B`.
    ///
    /// Returns the rendered guard string, or empty for [`Expr::Always`].
    pub fn render(&self) -> String {
        match self {
            Self::Always => String::new(),
            Self::Node(n) => n.name().to_owned(),
            Self::And(l, r) => format!("{} && {}", l.render(), r.render()),
            Self::Or(l, r) => format!("({} || {})", l.render(), r.render()),
            Self::Not(x) => match x {
                Self::Node(n) => format!("!{}", n.name()),
                other => format!("!({})", other.render()),
            },
        }
    }

    /// Collects `(name, type id)` for every node this expression references.
    ///
    /// - `out`: pairs are appended here, for [`crate::render::coverage`]'s
    ///   name-uniqueness check.
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

/// Declares a guard node as a unit struct plus its [`CondNode`] impl.
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

        impl $crate::machine::CondNode<$dom> for $name {
            fn name(&self) -> &'static str {
                stringify!($name)
            }
            fn eval(&self, $cx: &$crate::machine::Cx<'_, $dom>) -> $crate::machine::Cond {
                $body
            }
        }
    };
}

/// Builds a guard expression. Supports `&&` chains and a leading `!`.
///
/// `||` is deliberately omitted: supporting it complicates the macro rules well
/// out of proportion to how often it is needed. Use [`Expr::Or`] directly when
/// required.
#[macro_export]
macro_rules! check {
    () => { &$crate::machine::Expr::Always };
    (! $n:ident && $($rest:tt)*) => {
        &$crate::machine::Expr::And(
            &$crate::machine::Expr::Not(&$crate::machine::Expr::Node(&$n)),
            $crate::check!($($rest)*),
        )
    };
    ($n:ident && $($rest:tt)*) => {
        &$crate::machine::Expr::And(
            &$crate::machine::Expr::Node(&$n),
            $crate::check!($($rest)*),
        )
    };
    (! $n:ident) => { &$crate::machine::Expr::Not(&$crate::machine::Expr::Node(&$n)) };
    ($n:ident) => { &$crate::machine::Expr::Node(&$n) };
}
