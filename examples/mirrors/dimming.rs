//! Mirror dimming. A function of the current power state and gear, so it keeps
//! no state either.

use chart::feature::{Feature, FeatureInfo};

use crate::{Action, Event, Kind, Mirrors, World};

#[derive(Default)]
pub struct Dimming;

impl Feature<Mirrors> for Dimming {
    const INFO: FeatureInfo<Mirrors> = FeatureInfo {
        name: "Dimming",
        handles: &[Kind::PowerChanged, Kind::GearChanged],
        emits: &[Action::DimmingOn, Action::DimmingOff],
    };

    fn handle(&mut self, ev: &Event, world: &World, out: &mut Vec<Action>) {
        let (power, gear) = match ev {
            Event::PowerChanged(on) => (*on, world.gear_reverse),
            Event::GearChanged(rev) => (world.power_on, *rev),
            _ => return,
        };
        out.push(if power && !gear {
            Action::DimmingOn
        } else {
            Action::DimmingOff
        });
    }
}
