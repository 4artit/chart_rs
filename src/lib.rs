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
//! layers, so a feature that grows states keeps the same declaration.
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

/// The set of types one controller works with.
///
/// Bundling them into a single trait keeps the whole framework generic over one
/// type parameter `D` instead of four. A controller that also has states
/// declares a [`MachineSpec`] naming this domain.
pub trait Domain: Sized + 'static {
    /// Event body, including payload.
    ///
    /// Declaring it with [`events!`] also generates the [`HasKind`] impl.
    ///
    /// `Debug` is required so that the dispatch log carries the payload. An action
    /// that reads a runtime value out of `ev` is named but not valued in
    /// [`machine::Taken::actions`], and the log line is what closes that gap.
    type Event: HasKind<Kind = Self::EventKind> + Debug;

    /// Event kind: a payload-free tag. Edges match on this.
    ///
    /// Declaring it with [`events!`] also generates the [`Enumerable`] impl.
    type EventKind: Enumerable;

    /// An effect. Must be plain data so its name appears in logs and diagrams.
    type Action: Copy + Debug + 'static;

    /// The outside world (APIs and storage).
    ///
    /// Not required to be `Sized`, so a trait object may be used to narrow what
    /// this controller can see: `type Env = dyn MirrorWorld`.
    type Env: ?Sized;

    /// Carries out one action.
    ///
    /// This is the only place `Env` can be mutated; guards receive `&Env`. Every
    /// change this controller makes to the world therefore passes through an
    /// [`Domain::Action`] value that is recorded in [`machine::Taken::actions`] and drawn
    /// by [`render::to_mermaid`].
    ///
    /// `ev` is the event being dispatched. An action may read values from it that
    /// cannot be baked into an action list — those are `&'static`, so they hold
    /// compile-time constants only. State-dependent values therefore live in
    /// `Env`, initialised by a [`machine::State::entry`] action and cleared by a
    /// [`machine::State::exit`] one.
    ///
    /// `Machine` is not reachable from here, so an action cannot trigger a
    /// transition. Follow-up events are decided by the caller from the returned
    /// [`machine::Taken`].
    fn perform(action: Self::Action, ev: &Self::Event, world: &mut Self::Env);

    /// Every event kind, for gap checking.
    ///
    /// The default uses [`Enumerable::ALL`].
    fn all_kinds() -> &'static [Self::EventKind] {
        <Self::EventKind as Enumerable>::ALL
    }
}

/// One state machine's shape: the [`Domain`] it belongs to and what its states
/// are.
///
/// Kept apart from `Domain` for two reasons. A controller with no states — one
/// built from [`feature::Feature`] alone — never has to invent a tag enum. And a
/// controller that needs *several* machines declares one of these per machine
/// while they all share the same events, actions, guards and
/// [`Domain::perform`].
pub trait MachineSpec: Sized + 'static {
    /// The vocabulary this machine works in.
    type Domain: Domain;

    /// State identifier. Payloads live in [`Domain::Event`]; only the tag is here.
    ///
    /// Declaring it with [`tags!`] also generates the [`Enumerable`] impl.
    type Tag: Enumerable;

    /// Every state, for coverage checking.
    ///
    /// The default uses [`Enumerable::ALL`]. Override only to check a subset.
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
/// The world type of `M`'s domain.
pub type EnvOf<M> = <<M as MachineSpec>::Domain as Domain>::Env;
