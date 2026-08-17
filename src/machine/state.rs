//! States: a tag plus the actions that run on entering and leaving it.

use crate::{ActionOf, MachineSpec};

/// One state.
///
/// A state declares effects but **never transition logic** — transitions belong
/// in the [`super::Edge`] table so that the table alone describes the machine's
/// structure.
///
/// Like [`super::Edge`], this is static data with no behaviour of its own. A value
/// that only some states care about lives in [`crate::Domain::Env`], written by the
/// actions declared here, so that every change is named in
/// [`super::Taken::actions`] and drawn by [`crate::render::to_mermaid`].
pub struct State<M: MachineSpec> {
    pub tag: M::Tag,
    /// Actions run on entry, in declaration order.
    pub entry: &'static [ActionOf<M>],
    /// Actions run on exit, in declaration order.
    pub exit: &'static [ActionOf<M>],
}
