//! A declarative controller framework.
//!
//! A controller declares what it reacts to and what it does about it as static
//! data. Execution, diagram generation and exhaustive gap checking are all
//! derived from that one declaration, so it is the only place the logic lives.
//!
//! # Layers
//!
//! | Module | For |
//! |---|---|
//! | [`feature`] | Controllers with no states: what each feature reacts to and emits |
//! | [`machine`] | Controllers with states: a transition table and its executor |
//! | [`render`] | Diagrams and gap reports derived from either declaration |
//!
//! [`Domain`] bundles the types a controller works with and is shared by both
//! layers, so a feature that grows states keeps the same declaration. It holds
//! two effect vocabularies: [`Domain::Action`] for reactions to an event, and
//! [`Domain::StateAction`] for entry and exit effects, which never see one.
//!
//! # Declaration macros
//!
//! | Macro | Generates |
//! |---|---|
//! | [`tags!`] | State tag enum + [`Enumerable`] |
//! | [`events!`] | Event enum + kind enum + [`HasKind`] + [`Enumerable`] |
//! | [`cond_node!`] | A [`machine::CondNode`] impl |
//! | [`check!`] | A guard [`machine::Expr`] tree |

mod enums;

pub mod feature;
pub mod machine;
pub mod render;

#[cfg(test)]
mod tests;

pub use enums::{Enumerable, HasKind};

use std::fmt::Debug;

/// The set of types one controller works with: events, actions, and the
/// outside world. Every other item in this crate is generic over `D: Domain`.
pub trait Domain: Sized + 'static {
    /// Event body, including payload. [`events!`] generates this together with
    /// its [`HasKind`] impl.
    type Event: HasKind<Kind = Self::EventKind> + Debug;

    /// Payload-free event tag that edges match on. [`events!`] generates this
    /// together with its [`Enumerable`] impl.
    type EventKind: Enumerable;

    /// An effect produced in reaction to an event: what [`machine::Edge::run`]
    /// and [`feature::FeatureInfo::emits`] hold.
    type Action: Copy + Debug + 'static;

    /// An effect of being in a state: what [`machine::State::entry`] and
    /// [`machine::State::exit`] hold. They run whichever edge led there, so
    /// they cannot read the event — an effect that needs it is a
    /// [`Domain::Action`] on that edge.
    type StateAction: Copy + Debug + 'static;

    /// The outside world this controller reads and changes (APIs, storage).
    /// `?Sized` so a trait object can narrow it, e.g. `type Env = dyn Foo`.
    type Env: ?Sized;

    /// Carries out one action. With [`Domain::perform_state`], the only place
    /// `Env` may be mutated — guards only ever see `&Env`.
    ///
    /// - `action`: the effect to carry out.
    /// - `ev`: the event being dispatched, for actions that need a runtime
    ///   value from its payload.
    /// - `world`: the outside world to mutate.
    fn perform(action: Self::Action, ev: &Self::Event, world: &mut Self::Env);

    /// Carries out one entry or exit effect. No event: see
    /// [`Domain::StateAction`].
    ///
    /// - `action`: the effect to carry out.
    /// - `world`: the outside world to mutate.
    fn perform_state(action: Self::StateAction, world: &mut Self::Env);

    /// Every event kind, for [`render::coverage`]. Defaults to
    /// [`Enumerable::ALL`]; override only to check a subset.
    fn all_kinds() -> &'static [Self::EventKind] {
        <Self::EventKind as Enumerable>::ALL
    }
}

/// An effect vocabulary with no values, for a domain that uses only the other
/// one.
///
/// ```ignore
/// type Action = chart::NoAction;
///
/// fn perform(action: NoAction, _ev: &Event, _world: &mut Env) {
///     match action {}
/// }
/// ```
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum NoAction {}

/// One state machine's shape: which [`Domain`] it belongs to and what its
/// states are. Kept separate from `Domain` so a controller can declare
/// several machines sharing one domain, or none at all.
pub trait MachineSpec: Sized + 'static {
    /// The vocabulary this machine works in.
    type Domain: Domain;

    /// State identifier. [`tags!`] generates this together with its
    /// [`Enumerable`] impl.
    type Tag: Enumerable;

    /// Every state, for [`render::coverage`]. Defaults to [`Enumerable::ALL`];
    /// override only to check a subset.
    fn all_tags() -> &'static [Self::Tag] {
        <Self::Tag as Enumerable>::ALL
    }
}

/// The event type of `M`'s domain.
pub type EventOf<M> = <<M as MachineSpec>::Domain as Domain>::Event;
/// The event kind type of `M`'s domain.
pub type KindOf<M> = <<M as MachineSpec>::Domain as Domain>::EventKind;
/// The action type of `M`'s domain.
pub type ActionOf<M> = <<M as MachineSpec>::Domain as Domain>::Action;
/// The state action type of `M`'s domain.
pub type StateActionOf<M> = <<M as MachineSpec>::Domain as Domain>::StateAction;
/// The world type of `M`'s domain.
pub type EnvOf<M> = <<M as MachineSpec>::Domain as Domain>::Env;
