//! States: a tag plus the actions that run on entering and leaving it.

use super::Domain;

/// One state.
///
/// A state declares effects but **never transition logic** — transitions belong
/// in the [`super::Edge`] table so that the table alone describes the machine's
/// structure.
///
/// Like [`super::Edge`], this is static data with no behaviour of its own. A value
/// that only some states care about lives in [`Domain::Env`], written by the
/// actions declared here, so that every change is named in
/// [`super::Taken::actions`] and drawn by [`super::render::to_mermaid`].
pub struct State<D: Domain> {
    pub tag: D::Tag,
    /// Actions run on entry, in declaration order.
    pub entry: &'static [D::Action],
    /// Actions run on exit, in declaration order.
    pub exit: &'static [D::Action],
}
