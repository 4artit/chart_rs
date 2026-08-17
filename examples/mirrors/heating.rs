//! Mirror heating. Follows the defog signal, so it keeps no state.

use chart::feature::{Feature, FeatureInfo};

use crate::{Action, Event, Kind, Mirrors, World};

#[derive(Default)]
pub struct Heating;

impl Feature<Mirrors> for Heating {
    const INFO: FeatureInfo<Mirrors> = FeatureInfo {
        name: "Heating",
        handles: &[Kind::DefogChanged],
        emits: &[Action::HeatingOn, Action::HeatingOff],
    };

    fn handle(&mut self, ev: &Event, _world: &World, out: &mut Vec<Action>) {
        if let Event::DefogChanged(on) = ev {
            out.push(if *on {
                Action::HeatingOn
            } else {
                Action::HeatingOff
            });
        }
    }
}
