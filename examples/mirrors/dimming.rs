//! 방현(dimming) — 상태 없음.
//!
//! 전원과 기어, 두 현재 값의 순수 함수다. 같은 입력이면 언제나 같은 결과라
//! 상태 기계로 만들 이유가 없다.

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
