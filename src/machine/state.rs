//! States: a tag plus the actions that run on entering and leaving it.

use crate::{MachineSpec, StateActionOf};

/// One state: a tag plus its entry/exit actions. Static data only —
/// transition logic belongs in the [`super::Edge`] table, not here.
///
/// They hold [`crate::Domain::StateAction`], from which the event is not
/// reachable; an effect that reads it belongs in [`super::Edge::run`].
pub struct State<M: MachineSpec> {
    pub tag: M::Tag,
    /// Actions run on entry, in declaration order.
    pub entry: &'static [StateActionOf<M>],
    /// Actions run on exit, in declaration order.
    pub exit: &'static [StateActionOf<M>],
}
