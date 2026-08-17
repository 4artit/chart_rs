//! 난방 — 상태 없음.
//!
//! 디포그 신호를 그대로 따라가므로 기억할 것이 없다. `feature` 층이면 충분하다.

use chart::feature::{Feature, FeatureInfo};

use crate::{Action, Event, Mirrors, World};

#[derive(Default)]
pub struct Heating;

impl Feature<Mirrors> for Heating {
    const INFO: FeatureInfo<Mirrors> = FeatureInfo {
        name: "Heating",
        handles: &[crate::Kind::DefogChanged],
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
