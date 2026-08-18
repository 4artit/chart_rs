//! The stateless layer: one feature of a controller, declaring what it reacts to
//! and what it does about it.

use crate::{Domain, Enumerable, HasKind};

/// What a feature reacts to and what it emits.
///
/// A plain value rather than associated constants, so features can be
/// collected into a slice for [`crate::render::io_table`] (associated
/// constants aren't object-safe).
pub struct FeatureInfo<D: Domain> {
    /// Display name, used in tables and diagrams.
    pub name: &'static str,
    /// Event kinds this feature reacts to.
    pub handles: &'static [D::EventKind],
    /// Actions this feature may emit.
    pub emits: &'static [D::Action],
}

/// One feature of a controller: what it reacts to and what it emits.
pub trait Feature<D: Domain> {
    /// This feature's declared inputs and outputs.
    const INFO: FeatureInfo<D>;

    /// Reacts to `ev` by pushing effects onto `out`, rather than carrying them
    /// out directly — [`dispatch`] checks each one against
    /// [`FeatureInfo::emits`] before [`Domain::perform`] runs it.
    ///
    /// - `ev`: the event to react to.
    /// - `world`: the outside world, read-only.
    /// - `out`: effects to append, in the order they should run.
    fn handle(&mut self, ev: &D::Event, world: &D::Env, out: &mut Vec<D::Action>);
}

/// Runs `f` on `ev` if `f` declared this event's kind, then debug-checks that
/// it only emitted actions it declared.
///
/// - `f`: the feature to run.
/// - `ev`: the event to dispatch.
/// - `world`: the outside world, read-only.
/// - `out`: effects `f` emits are appended here.
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

/// Event kinds nothing in the controller accounts for.
///
/// - `features`: the controller's feature list.
/// - `elsewhere`: kinds handled outside that list, e.g.
///   [`crate::render::handled_kinds`] for each state machine the controller also
///   runs; pass `&[]` if there are none.
///
/// Returns the event kinds handled by neither `features` nor `elsewhere`.
pub fn unhandled_kinds<D: Domain>(
    features: &[FeatureInfo<D>],
    elsewhere: &[&[D::EventKind]],
) -> Vec<D::EventKind> {
    D::all_kinds()
        .iter()
        .copied()
        .filter(|k| !features.iter().any(|f| f.handles.contains(k)))
        .filter(|k| !elsewhere.iter().any(|ks| ks.contains(k)))
        .collect()
}

/// Actions no feature emits — dead unless the machine layer runs them.
///
/// - `features`: the controller's feature list.
///
/// Returns every action not listed in any feature's [`FeatureInfo::emits`].
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
