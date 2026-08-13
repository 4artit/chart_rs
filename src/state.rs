//! State nodes: a state tag plus the variables scoped to that state.

use std::any::Any;

use super::Domain;

/// One state.
///
/// A state may own variables and define entry/exit effects, but **never
/// transition logic** — transitions belong in the [`super::Edge`] table so that
/// the table alone describes the machine's structure.
///
/// `Machine` builds every state node up front and keeps it for its own lifetime;
/// nodes are not created or dropped on transition. Resetting variables in
/// `on_exit` is therefore what makes them genuinely state-scoped.
pub trait StateNode<D: Domain>: Any {
    fn tag(&self) -> D::Tag;

    /// Pushes the effects of entering onto `out`. Does not touch the world
    /// directly.
    fn on_enter(&mut self, _ev: &D::Event, _world: &D::Env, _out: &mut Vec<D::Action>) {}

    /// Pushes the effects of leaving onto `out` and resets state-scoped variables.
    fn on_exit(&mut self, _world: &D::Env, _out: &mut Vec<D::Action>) {}

    /// Downcast hook for [`super::Cx::state_as`]. The [`state!`] macro generates
    /// this.
    fn as_any(&self) -> &dyn Any;
}

/// Declares a state that owns no variables.
///
/// ```ignore
/// state!(RearCam, Off,     tag: Tag::Off);
/// state!(RearCam, Showing, tag: Tag::Showing,
///        on_enter: [Action::ShowCamera],
///        on_exit:  [Action::HideCamera]);
/// ```
///
/// A state that needs variables implements [`StateNode`] directly.
#[macro_export]
macro_rules! state {
    ($dom:ty, $name:ident, tag: $tag:expr) => {
        $crate::state!($dom, $name, tag: $tag, on_enter: [], on_exit: []);
    };
    ($dom:ty, $name:ident, tag: $tag:expr, on_enter: [$($enter:expr),* $(,)?]) => {
        $crate::state!($dom, $name, tag: $tag, on_enter: [$($enter),*], on_exit: []);
    };
    ($dom:ty, $name:ident, tag: $tag:expr, on_exit: [$($exit:expr),* $(,)?]) => {
        $crate::state!($dom, $name, tag: $tag, on_enter: [], on_exit: [$($exit),*]);
    };
    ($dom:ty, $name:ident, tag: $tag:expr,
     on_enter: [$($enter:expr),* $(,)?], on_exit: [$($exit:expr),* $(,)?]) => {
        #[derive(Default)]
        pub struct $name;

        impl $crate::StateNode<$dom> for $name {
            fn tag(&self) -> <$dom as $crate::Domain>::Tag {
                $tag
            }
            fn on_enter(
                &mut self,
                _ev: &<$dom as $crate::Domain>::Event,
                _world: &<$dom as $crate::Domain>::Env,
                out: &mut Vec<<$dom as $crate::Domain>::Action>,
            ) {
                let _ = &out; // silences the unused warning for empty action lists
                $(out.push($enter);)*
            }
            fn on_exit(
                &mut self,
                _world: &<$dom as $crate::Domain>::Env,
                out: &mut Vec<<$dom as $crate::Domain>::Action>,
            ) {
                let _ = &out;
                $(out.push($exit);)*
            }
            fn as_any(&self) -> &dyn ::std::any::Any {
                self
            }
        }
    };
}
