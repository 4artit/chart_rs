//! The stateless layer: one feature of a controller, declaring what it reacts to
//! and what it does about it.
//!
//! A controller whose behaviour does not depend on history needs no transition
//! table, but it still benefits from having its inputs and effects written down
//! as data. [`Feature::INFO`] is that declaration, and
//! [`crate::render::io_table`] turns it into documentation.
//!
//! The declaration is load-bearing rather than descriptive: [`dispatch`] only
//! calls a feature for the kinds it lists, so a handler that reacts to something
//! it did not declare never runs at all.

use crate::{Domain, Enumerable, HasKind};

/// What a feature reacts to and what it emits.
///
/// Held as a value rather than as associated constants so that features can be
/// listed together for [`crate::render::io_table`]; associated constants are not
/// object safe.
pub struct FeatureInfo<D: Domain> {
    pub name: &'static str,
    pub handles: &'static [D::EventKind],
    pub emits: &'static [D::Action],
}

/// One feature of a controller.
pub trait Feature<D: Domain> {
    const INFO: FeatureInfo<D>;

    /// Reacts to `ev`, pushing effects onto `out` rather than carrying them out.
    ///
    /// Deferring the effects is what lets [`dispatch`] check them against
    /// [`FeatureInfo::emits`], and keeps [`Domain::perform`] the single place the
    /// world is touched.
    fn handle(&mut self, ev: &D::Event, world: &D::Env, out: &mut Vec<D::Action>);
}

/// Runs `f` if it declared this event's kind, then checks what it emitted.
///
/// Filtering here is what keeps [`FeatureInfo::handles`] honest: a kind left out
/// of the declaration never reaches the handler, so the omission shows up as
/// missing behaviour rather than as stale documentation.
pub fn dispatch<D, F>(f: &mut F, ev: &D::Event, world: &D::Env, out: &mut Vec<D::Action>)
where
    D: Domain,
    D::Action: PartialEq,
    F: Feature<D>,
{
    if !F::INFO.handles.contains(&ev.kind()) {
        return;
    }

    let first = out.len();
    f.handle(ev, world, out);

    debug_assert!(
        out[first..].iter().all(|a| F::INFO.emits.contains(a)),
        "{}: emitted an action it does not declare -> {:?}",
        F::INFO.name,
        &out[first..],
    );
}

/// Event kinds no feature handles. The [`crate::render::coverage`] `holes` of
/// this layer.
pub fn unhandled_kinds<D: Domain>(features: &[FeatureInfo<D>]) -> Vec<D::EventKind> {
    D::all_kinds()
        .iter()
        .copied()
        .filter(|k| !features.iter().any(|f| f.handles.contains(k)))
        .collect()
}

/// Actions no feature emits, which are dead unless the machine layer runs them.
pub fn unemitted_actions<D: Domain>(features: &[FeatureInfo<D>]) -> Vec<D::Action>
where
    D::Action: Enumerable,
{
    <D::Action as Enumerable>::ALL
        .iter()
        .copied()
        .filter(|a| !features.iter().any(|f| f.emits.contains(a)))
        .collect()
}
