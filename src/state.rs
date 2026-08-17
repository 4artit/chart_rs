//! State nodes: a state tag plus the variables scoped to that state.

use std::any::Any;

use super::Domain;

/// One state.
///
/// A state may own variables and define entry/exit effects, but **never
/// transition logic** — transitions belong in the [`super::Edge`] table so that
/// the table alone describes the machine's structure.
///
/// Effects are declared as static data; the hooks manage variables only.
///
/// `Machine` builds every state node up front and keeps it for its own lifetime;
/// nodes are not created or dropped on transition. Resetting variables in
/// `on_exit` is therefore what makes them genuinely state-scoped.
pub trait StateNode<D: Domain>: Any {
    fn tag(&self) -> D::Tag;

    /// Actions run on entry.
    fn entry_actions(&self) -> &'static [D::Action] {
        &[]
    }

    /// Actions run on exit.
    fn exit_actions(&self) -> &'static [D::Action] {
        &[]
    }

    /// Initialises state-scoped variables from the event.
    fn on_enter(&mut self, _ev: &D::Event, _world: &D::Env) {}

    /// Resets state-scoped variables.
    fn on_exit(&mut self, _world: &D::Env) {}

    /// Downcast hook for [`super::Cx::state_as`]. The [`state!`] macro generates
    /// this.
    fn as_any(&self) -> &dyn Any;
}

/// Declares a state that owns no variables.
///
/// ```ignore
/// state!(RearCam, Off,     tag: Tag::Off);
/// state!(RearCam, Showing, tag: Tag::Showing,
///        entry: [Action::ShowCamera],
///        exit:  [Action::HideCamera]);
/// ```
///
/// The lists become [`StateNode::entry_actions`] and [`StateNode::exit_actions`];
/// their elements must be constants. A state that needs variables implements
/// [`StateNode`] directly.
#[macro_export]
macro_rules! state {
    ($dom:ty, $name:ident, tag: $tag:expr) => {
        $crate::state!($dom, $name, tag: $tag, entry: [], exit: []);
    };
    ($dom:ty, $name:ident, tag: $tag:expr, entry: [$($enter:expr),* $(,)?]) => {
        $crate::state!($dom, $name, tag: $tag, entry: [$($enter),*], exit: []);
    };
    ($dom:ty, $name:ident, tag: $tag:expr, exit: [$($exit:expr),* $(,)?]) => {
        $crate::state!($dom, $name, tag: $tag, entry: [], exit: [$($exit),*]);
    };
    ($dom:ty, $name:ident, tag: $tag:expr,
     entry: [$($enter:expr),* $(,)?], exit: [$($exit:expr),* $(,)?]) => {
        #[derive(Default)]
        pub struct $name;

        impl $crate::StateNode<$dom> for $name {
            fn tag(&self) -> <$dom as $crate::Domain>::Tag {
                $tag
            }
            fn entry_actions(&self) -> &'static [<$dom as $crate::Domain>::Action] {
                &[$($enter),*]
            }
            fn exit_actions(&self) -> &'static [<$dom as $crate::Domain>::Action] {
                &[$($exit),*]
            }
            fn as_any(&self) -> &dyn ::std::any::Any {
                self
            }
        }
    };
}
