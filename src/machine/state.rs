//! States: a tag plus the actions that run on entering and leaving it.

use crate::{ActionOf, MachineSpec};

/// One state: a tag plus its entry/exit actions. Static data only —
/// transition logic belongs in the [`super::Edge`] table, not here.
pub struct State<M: MachineSpec> {
    pub tag: M::Tag,
    /// Actions run on entry, in declaration order.
    pub entry: &'static [ActionOf<M>],
    /// Actions run on exit, in declaration order.
    pub exit: &'static [ActionOf<M>],
}
