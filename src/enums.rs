//! Declaring enums together with the exhaustive list of their values.

use std::fmt::Debug;

/// A type whose values can all be listed at compile time.
///
/// Required by [`super::MachineSpec::Tag`] and [`super::Domain::EventKind`], whose
/// full value lists [`super::render::coverage`] needs to walk `(state × event
/// kind)` exhaustively. Implementing this by hand is supported, but nothing
/// then verifies `ALL` stays complete as variants are added — prefer [`tags!`]
/// or [`events!`], which generate the enum and `ALL` together.
pub trait Enumerable: Copy + Eq + Debug + 'static {
    /// Every value of this type.
    const ALL: &'static [Self];
}

/// A type that can report its payload-free kind tag.
///
/// Required by [`super::Domain::Event`]. [`events!`] generates a one-to-one impl;
/// implement it by hand to collapse several event variants into one kind:
///
/// ```ignore
/// impl chart::HasKind for Event {
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
/// Derives `Copy + Clone + PartialEq + Eq + Debug` on the enum and forwards
/// any outer attributes.
///
/// ```ignore
/// chart::tags! {
///     pub enum Tag {
///         Locked,
///         Unlocked,
///     }
/// }
/// ```
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
/// Generates four items: `enum Event` (the body with payloads, attributes
/// forwarded), `enum Kind` (the payload-free tag, with `Copy + Eq + Debug`
/// derived), `impl HasKind for Event`, and `impl Enumerable for Kind`.
///
/// ```ignore
/// chart::events! {
///     #[derive(Clone, Debug)]
///     pub enum Event => Kind {
///         EnterCode(u32),
///         Timeout,
///     }
/// }
/// ```
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
