//! 참고용 스케치 — 하나의 컨트롤러 안에서 기능을 파일로 나누고, 상태가 없는 것은
//! `feature`로, 있는 것은 `machine`으로 관리하는 방식. 커밋 대상이 아니다.
//!
//!     cargo run --example mirrors
//!
//! | 파일 | 층 | 이유 |
//! |---|---|---|
//! | `heating.rs` | feature | 디포그 신호를 그대로 따라간다. 기억이 없다 |
//! | `dimming.rs` | feature | 전원과 기어 두 현재 값의 순수 함수다 |
//! | `fold.rs` | machine | 접는 중/펴는 중이라는 진행 상태가 있다 |
//!
//! 신입이 폴드 티켓을 받으면 `fold.rs`만 읽으면 된다. 라우터(`handle_event`)는
//! 목차 역할만 하고 판단하지 않는다.

mod dimming;
mod fold;
mod heating;

use chart::feature::{self, Feature, FeatureInfo};
use chart::{Domain, HasKind, render};

use dimming::Dimming;
use fold::Fold;
use heating::Heating;

chart::events! {
    #[derive(Clone, Debug)]
    pub enum Event => Kind {
        DefogChanged(bool),
        PowerChanged(bool),
        GearChanged(bool),
        SpeedChanged(f32),
        /// 폴드 모터가 보고하는 실제 위치. 0.0 = 접힘, 1.0 = 펴짐.
        FoldPositionChanged(f32),
        UserChanged,
    }
}

/// payload를 싣지 않는다. 표에 이름만 찍히고, `machine` 층의 `&'static [Action]`
/// 제약에도 그대로 맞는다. 값이 필요하면 `perform`에서 `ev`나 world에서 꺼낸다.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Action {
    HeatingOn,
    HeatingOff,
    DimmingOn,
    DimmingOff,
    Fold,
    Unfold,
}

/// 바깥 세계. 실제로는 `ApiBridge` 자리.
#[derive(Default)]
pub struct World {
    pub power_on: bool,
    pub gear_reverse: bool,
    pub speed: f32,
    pub fold_position: f32,
    pub effects: Vec<String>,
}

/// 컨트롤러가 쓰는 타입 묶음. 상태 없는 기능들이 이걸 쓴다.
pub struct Mirrors;

impl Domain for Mirrors {
    type Event = Event;
    type EventKind = Kind;
    type Action = Action;
    type Env = World;

    /// 액션을 수행하는 유일한 지점. 두 층이 공유한다.
    fn perform(action: Action, _ev: &Event, world: &mut World) {
        let line = match action {
            Action::HeatingOn => "heating on".to_string(),
            Action::HeatingOff => "heating off".to_string(),
            Action::DimmingOn => "dimming on".to_string(),
            Action::DimmingOff => "dimming off".to_string(),
            Action::Fold => format!("fold (speed {:.0})", world.speed),
            Action::Unfold => "unfold".to_string(),
        };
        world.effects.push(line);
    }
}

// ─────────────────────────────────────────── 라우터

/// 등록 목록은 손으로 유지한다. 라우터 바로 옆에 두어 누락이 눈에 띄게 한다.
const FEATURES: &[FeatureInfo<Mirrors>] = &[Heating::INFO, Dimming::INFO];

#[derive(Default)]
struct Controller {
    heating: Heating,
    dimming: Dimming,
    fold: Fold,
}

impl Controller {
    fn handle_event(&mut self, ev: &Event, world: &mut World) {
        // 전역 게이트는 여기 한 곳에만. 걸러낸 이유를 남긴다.
        if !world.power_on && requires_power(ev.kind()) {
            println!("  (무시: 전원 꺼짐) {ev:?}");
            return;
        }

        // 상태 없는 층: 액션을 모아 라우터가 수행한다.
        let mut actions = Vec::new();
        feature::dispatch(&mut self.heating, ev, world, &mut actions);
        feature::dispatch(&mut self.dimming, ev, world, &mut actions);
        for &a in &actions {
            Mirrors::perform(a, ev, world);
        }

        // 상태 있는 층: 머신이 스스로 수행하고 결과를 돌려준다.
        let taken = self.fold.dispatch(ev, world);

        match (actions.is_empty(), taken) {
            (true, None) => println!("  -> (변화 없음)"),
            (false, None) => println!("  -> {actions:?}"),
            (true, Some(t)) => println!("  -> [fold:{}] {:?}", t.edge, t.actions),
            (false, Some(t)) => {
                println!("  -> {actions:?} + [fold:{}] {:?}", t.edge, t.actions)
            }
        }
    }
}

/// "어떤 이벤트가 전원을 요구하는가"가 흩어지지 않고 한 목록으로 남는다.
fn requires_power(kind: Kind) -> bool {
    matches!(kind, Kind::DefogChanged)
}

// ─────────────────────────────────────────── 실행

fn main() {
    let mut c = Controller::default();
    let mut w = World {
        fold_position: 1.0, // 펴진 상태로 시작
        ..Default::default()
    };

    let steps: &[(&str, Event)] = &[
        ("전원 꺼짐 상태에서 디포그", Event::DefogChanged(true)),
        ("전원 켜짐", Event::PowerChanged(true)),
        ("디포그 켜짐", Event::DefogChanged(true)),
        ("후진 기어", Event::GearChanged(true)),
        ("전원 꺼짐 -> 접기 시작", Event::PowerChanged(false)),
        ("모터가 절반쯤", Event::FoldPositionChanged(0.5)),
        ("모터가 다 접힘", Event::FoldPositionChanged(0.0)),
        ("전원 켜짐 -> 펴기 시작", Event::PowerChanged(true)),
        ("모터가 다 펴짐", Event::FoldPositionChanged(1.0)),
        ("사용자 변경(처리자 없음)", Event::UserChanged),
    ];

    println!("── 실행 ──");
    for (desc, ev) in steps {
        println!("{desc}");
        apply_signal(ev, &mut w);
        c.handle_event(ev, &mut w);
    }

    println!("\n수행된 효과: {:#?}", w.effects);

    println!("\n── 기능 입출력 표 ──\n{}", render::io_table(FEATURES));
    println!(
        "── 기능 흐름도 ──\n```mermaid\n{}```\n",
        render::io_flowchart(FEATURES)
    );
    println!(
        "── 폴드 상태 다이어그램 ──\n```mermaid\n{}```",
        fold::diagram()
    );

    println!("\n── 진단 ──");
    // 두 층을 합쳐서 본다. 폴드 머신이 다루는 이벤트는 구멍이 아니다.
    let by_fold = fold::handled_kinds();
    println!(
        "컨트롤러 전체에서 아무도 처리하지 않는 이벤트: {:?}",
        feature::unhandled_kinds(FEATURES, &[&by_fold])
    );
    let cov = fold::coverage();
    println!("폴드 표의 구멍: {:?}", cov.holes);
    println!("폴드 표가 깨끗한가: {}", cov.is_clean());
}

/// 콜백이 실어온 값을 world에 반영한다. 실제로는 상위 서비스가 하는 일.
fn apply_signal(ev: &Event, w: &mut World) {
    match ev {
        Event::PowerChanged(on) => w.power_on = *on,
        Event::GearChanged(rev) => w.gear_reverse = *rev,
        Event::SpeedChanged(v) => w.speed = *v,
        Event::FoldPositionChanged(p) => w.fold_position = *p,
        _ => {}
    }
}
