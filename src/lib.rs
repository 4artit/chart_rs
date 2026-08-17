//! A declarative finite state machine framework.
//!
//! Transitions are declared as a single `&'static [Edge<D>]` table. Execution,
//! diagram generation, and exhaustive coverage checking are all derived from
//! that one table, so the table is the only place transition logic lives.
//!
//! # Components
//!
//! | Item | Mutable state | Role |
//! |---|---|---|
//! | [`Domain`] | none | Type bundle for one controller (Tag / Event / Action / Env) |
//! | [`CondNode`] | none (`&self`) | Evaluates a transition guard |
//! | [`State`] | none (static) | One state's tag and entry/exit actions |
//! | [`Edge`] | none (static) | One row of the table |
//! | [`Machine`] | current tag | The executor |
//!
//! # Declaration macros
//!
//! | Macro | Generates |
//! |---|---|
//! | [`tags!`] | State tag enum + [`Enumerable`] |
//! | [`events!`] | Event enum + kind enum + [`HasKind`] + [`Enumerable`] |
//! | [`cond_node!`] | A [`CondNode`] impl |
//! | [`check!`] | A guard [`Expr`] tree |

mod cond;
mod edge;
mod enums;
mod machine;
mod node;
pub mod render;
mod state;

#[cfg(test)]
mod tests;

pub use cond::Cond;
pub use edge::{Edge, Goto, Ignore, OnUnknown, Source};
pub use enums::{Enumerable, HasKind};
pub use machine::{Machine, Taken};
pub use node::{CondNode, Cx, Expr, Memo};
pub use state::State;

use std::fmt::Debug;

/// The set of types one controller works with.
///
/// Bundling them into a single trait keeps the whole framework generic over one
/// type parameter `D` instead of five.
pub trait Domain: Sized + 'static {
    /// State identifier. Payloads live in [`Domain::Event`]; only the tag is here.
    ///
    /// Declaring it with [`tags!`] also generates the [`Enumerable`] impl.
    type Tag: Enumerable;

    /// Event body, including payload.
    ///
    /// Declaring it with [`events!`] also generates the [`HasKind`] impl.
    ///
    /// `Debug` is required so that the dispatch log carries the payload. An action
    /// that reads a runtime value out of `ev` is named but not valued in
    /// [`Taken::actions`], and the log line is what closes that gap.
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
    /// [`Domain::Action`] value that is recorded in [`Taken::actions`] and drawn
    /// by [`render::to_mermaid`].
    ///
    /// `ev` is the event being dispatched. An action may read values from it that
    /// cannot be baked into an action list — those are `&'static`, so they hold
    /// compile-time constants only. State-dependent values therefore live in
    /// `Env`, initialised by a [`State::entry`] action and cleared by a
    /// [`State::exit`] one.
    ///
    /// `Machine` is not reachable from here, so an action cannot trigger a
    /// transition. Follow-up events are decided by the caller from the returned
    /// [`Taken`].
    fn perform(action: Self::Action, ev: &Self::Event, world: &mut Self::Env);

    /// Every state, for coverage checking.
    ///
    /// The default uses [`Enumerable::ALL`]. Override only to check a subset.
    fn all_tags() -> &'static [Self::Tag] {
        <Self::Tag as Enumerable>::ALL
    }

    /// Every event kind, for coverage checking.
    ///
    /// The default uses [`Enumerable::ALL`].
    fn all_kinds() -> &'static [Self::EventKind] {
        <Self::EventKind as Enumerable>::ALL
    }
}
