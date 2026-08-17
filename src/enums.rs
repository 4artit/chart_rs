//! Declaring enums together with the exhaustive list of their values.

use std::fmt::Debug;

/// A type whose values can be enumerated.
///
/// [`super::render::coverage`] walks `(state × event kind)` exhaustively, which
/// requires the full value list of both axes. A hand-maintained list is the one
/// place where forgetting an entry is silent: the combination simply drops out of
/// the check. [`tags!`] and [`events!`] emit the enum and its `ALL` from the same
/// declaration so the two cannot drift.
///
/// Implementing this by hand is supported; nothing then verifies that `ALL` is
/// complete.
pub trait Enumerable: Copy + Eq + Debug + 'static {
    /// Every value of this type.
    const ALL: &'static [Self];
}

/// A type that can report its payload-free kind tag.
///
/// [`super::Domain::Event`] requires this, so the event-to-kind mapping exists in
/// exactly one place. [`events!`] generates a one-to-one mapping.
///
/// Implement it by hand to collapse several event variants into one kind:
///
/// ```ignore
/// impl fsm::HasKind for Event {
///     type Kind = Kind;
///     fn kind(&self) -> Kind {
///         match self {
///             Event::TiltUp | Event::TiltDown => Kind::TiltAdjust,
///             Event::Gear(_) => Kind::Gear,
///         }
///     }
/// }
/// ```
///
/// If the event type comes from another crate the orphan rule forbids a direct
/// impl; wrap it in a newtype and implement this for the wrapper.
pub trait HasKind {
    /// The payload-free kind tag that edges match on.
    type Kind: Enumerable;

    /// Reports this event's kind.
    fn kind(&self) -> Self::Kind;
}

/// Declares a state tag enum together with its [`Enumerable`] impl.
///
/// ```ignore
/// fsm::tags! {
///     pub enum Tag {
///         Locked,
///         Unlocked,
///     }
/// }
/// ```
///
/// The `Copy + Eq + Debug` that [`super::StateDomain::Tag`] requires are derived
/// automatically. Outer attributes are forwarded to the enum.
#[macro_export]
macro_rules! tags {
    (
        $(#[$meta:meta])*
        $vis:vis enum $Name:ident {
            $(
                $(#[$vmeta:meta])*
                $variant:ident
            ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Copy, Clone, PartialEq, Eq, Debug)]
        $vis enum $Name {
            $(
                $(#[$vmeta])*
                $variant,
            )*
        }

        impl $crate::Enumerable for $Name {
            const ALL: &'static [Self] = &[$(Self::$variant),*];
        }
    };
}

/// Declares an event enum and its kind enum from a single list of variants.
///
/// ```ignore
/// fsm::events! {
///     #[derive(Clone, Debug)]
///     pub enum Event => Kind {
///         EnterCode(u32),
///         Timeout,
///     }
/// }
/// ```
///
/// Generates four items:
///
/// - `enum Event` — the body with payloads. Attributes are forwarded.
/// - `enum Kind` — the payload-free tag, with `Copy + Eq + Debug` derived.
/// - `impl HasKind for Event` — used by the executor to classify events.
/// - `impl Enumerable for Kind` — used by [`super::Domain::all_kinds`].
///
/// Written by hand these are four places that must agree; if only the value list
/// drifts, coverage checking silently narrows.
#[macro_export]
macro_rules! events {
    (
        $(#[$meta:meta])*
        $vis:vis enum $Event:ident => $Kind:ident {
            $(
                $(#[$vmeta:meta])*
                $variant:ident $( ( $($ty:ty),* $(,)? ) )?
            ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis enum $Event {
            $(
                $(#[$vmeta])*
                $variant $( ( $($ty),* ) )?,
            )*
        }

        #[derive(Copy, Clone, PartialEq, Eq, Debug)]
        $vis enum $Kind {
            $(
                $(#[$vmeta])*
                $variant,
            )*
        }

        impl $crate::HasKind for $Event {
            type Kind = $Kind;

            fn kind(&self) -> $Kind {
                match self {
                    // `Variant { .. }` matches unit and tuple variants alike, so
                    // no separate expansion is needed for payload-carrying ones.
                    $(Self::$variant { .. } => $Kind::$variant,)*
                }
            }
        }

        impl $crate::Enumerable for $Kind {
            const ALL: &'static [Self] = &[$(Self::$variant),*];
        }
    };
}
