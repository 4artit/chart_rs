# chart

선언형 컨트롤러 프레임워크. 컨트롤러가 무엇에 반응하고 무엇을 하는지를
정적 데이터(상태 전이 표, 또는 단순한 입출력 목록)로 선언하면, 그 선언
하나가 실행·mermaid 다이어그램·전수 누락 검사를 모두 만들어낸다.

[English README](README.md)

## 왜 선언인가

로직을 `match` 문에 흩어 두면 "이 이벤트가 오면 무슨 일이 일어나는가"를
파악하려고 코드를 전부 읽어야 하고, 누군가 그려둔 다이어그램은 시간이
지나면서 실제와 어긋난다. chart는 반대로 접근한다. 전이 표(또는 기능
목록) 자체가 소스이고, 다이어그램과 커버리지 검사는 실행기가 실제로 읽는
그 데이터에서 그대로 생성된다. 동기화할 사본이 애초에 하나뿐이다.

## 두 개의 층, 하나의 어휘

| 층 | 쓰는 경우 | 선언하는 것 |
|---|---|---|
| `feature` | 동작이 과거에 의존하지 않음 | 기능마다 받는 이벤트와 내는 액션 |
| `machine` | 같은 이벤트가 상태에 따라 다른 의미를 가짐 | 전이 표 |

두 층은 `Domain`(이벤트·액션·바깥 세상 타입의 묶음)을 공유한다. 그래서
상태 없이 시작한 기능이 나중에 이력이 필요해져도 선언은 그대로 두고
작은 `MachineSpec`만 옆에 추가하면 된다. 대부분의 컨트롤러는 `feature`로
충분하고, 정말 필요한 곳에만 `machine`을 쓴다.

## 설치

crates.io에 배포되지 않는다. 로컬 경로 의존성으로 쓴다.

```toml
[dependencies]
chart = { path = "../chart" }
```

## 빠른 시작

전등을 껐다 켰다 하는 2상태 예제:

```rust
use chart::machine::{Edge, Goto, Ignore, Machine, OnUnknown, Source, State};
use chart::{Domain, MachineSpec};

chart::tags! { enum Tag { Off, On } }
chart::events! {
    #[derive(Clone, Debug)]
    enum Event => Kind { Toggle }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Action { TurnOn, TurnOff }

struct Env;
struct Light;

impl Domain for Light {
    type Event = Event;
    type EventKind = Kind;
    type Action = Action;
    type Env = Env;

    fn perform(action: Action, _ev: &Event, _world: &mut Env) {
        match action {
            Action::TurnOn => println!("on"),
            Action::TurnOff => println!("off"),
        }
    }
}

impl MachineSpec for Light {
    type Domain = Light;
    type Tag = Tag;
}

static STATES: &[State<Light>] = &[
    State { tag: Tag::Off, entry: &[Action::TurnOff], exit: &[] },
    State { tag: Tag::On,  entry: &[Action::TurnOn],  exit: &[] },
];

static EDGES: &[Edge<Light>] = &[
    Edge { id: "TURN_ON",  from: Source::These(&[Tag::Off]), when: Kind::Toggle,
           check: chart::check!(), unknown: OnUnknown::Deny, run: &[], goto: Goto::To(Tag::On) },
    Edge { id: "TURN_OFF", from: Source::These(&[Tag::On]),  when: Kind::Toggle,
           check: chart::check!(), unknown: OnUnknown::Deny, run: &[], goto: Goto::To(Tag::Off) },
];

static IGNORES: &[Ignore<Light>] = &[];

fn main() {
    let mut world = Env;
    let mut m = Machine::new(Tag::Off, STATES, EDGES, IGNORES);
    m.dispatch(&Event::Toggle, &mut world); // -> on
    m.dispatch(&Event::Toggle, &mut world); // -> off
}
```

더 큰 예제:
- [`examples/door_lock`](examples/door_lock/main.rs) — 상태 4개, 조건
  가드, 와일드카드 `Ignore`. `cargo run --example door_lock`.
- [`examples/mirrors`](examples/mirrors/main.rs) — 두 층을 섞은 컨트롤러:
  상태 없는 기능 두 개와 상태 기계 하나. `cargo run --example mirrors`.

## 코드 대신 선언해서 얻는 것

- **실행기.** `Machine::dispatch`(또는 `feature::dispatch`)가 작성한 표를
  그대로 읽는다 — 표와 어긋날 수 있는 별도의 해석 단계가 없다.
- **다이어그램.** `render::to_mermaid`가 전이 표를 `stateDiagram-v2`로
  뽑아내고, `scripts/mermaid_to_plantuml.sh`로 PlantUML로도 바꿀 수 있다.
- **누락 검사.** `render::coverage`가 `(상태, 이벤트)` 조합을 전수
  순회해 엣지도 `Ignore`도 없는 것을 찾아낸다. 테스트에서 `is_clean()`을
  assert 해 두면, 빠뜨린 케이스가 운영 환경이 아니라 CI에서 걸린다.
- **실패를 인정하는 조건.** 조건 판정은 `bool`이 아니라
  `True`/`False`/`Unknown` 세 값이고, 판정 불가일 때의 정책은
  `Edge::unknown`에 명시된다 — 가드 함수 안에 숨는 대신 다이어그램에
  드러난다.
- **추적 가능한 부수효과.** 바깥 세상은 `Domain::perform`에서만 바뀐다.
  그래서 한 번의 dispatch가 만든 모든 효과는 로그로 남기거나 검증할 수
  있는 평범한 `Action` 값이다.

## 프로젝트 구조

```
src/
  lib.rs          // Domain, MachineSpec — 라이브러리 진입점
  feature.rs      // 상태 없는 층: Feature, FeatureInfo, dispatch
  machine.rs      // 상태 있는 층: Machine, Taken
  machine/        // Cond, CondNode, State, Edge, Source, Goto, OnUnknown
  render.rs       // to_mermaid, coverage, io_table, io_flowchart
examples/
  door_lock/      // cargo run --example door_lock
  mirrors/        // cargo run --example mirrors
```

`Domain`/`MachineSpec` 계약, 가드 작성법, dispatch 실행 순서 같은 상세
API 설명은 타입 자체에 문서로 달려 있다. `cargo doc --open`으로 확인한다.

## 테스트

```sh
cargo test
```
