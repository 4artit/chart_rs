# fsm

선언형 FSM(유한 상태 기계) 프레임워크. 컨트롤러의 전이를 `&'static [Edge<D>]` 표
하나로 선언하면, 그 표 하나에서 **실행**·**mermaid 다이어그램**·**전수 커버리지
검사**를 모두 얻는다.

## 왜 표 하나인가

전이 로직을 `match` 문 여기저기에 흩어 두면 "이 상태에서 이 이벤트가 오면
무슨 일이 일어나는가"를 파악하려고 코드를 전부 읽어야 한다. fsm은 그 대신
전이를 **데이터**(`Edge` 배열)로 선언하게 강제한다. 그러면:

- 실행기(`Machine`)가 표를 그대로 읽어서 동작한다.
- 같은 표에서 mermaid 상태 다이어그램을 자동 생성한다.
- 같은 표에서 "(상태 × 이벤트) 조합 중 처리되지 않은 게 있는가"를 기계적으로
  검사한다 — 빠뜨린 케이스는 버그가 아니라 컴파일 타임/테스트 타임에 잡히는
  구멍(hole)이 된다.

## 설치

crates.io에 배포된 패키지는 아니다. 로컬 경로 의존성으로 쓴다.

```toml
[dependencies]
fsm = { path = "../fsm" }
```

## 핵심 개념

| 요소 | 내부 상태 | 역할 |
|---|---|---|
| `Domain` | — | 컨트롤러가 쓸 타입 묶음 (Tag / Event / Action / Env) |
| `CondNode` | 없음 (`&self`) | 전이 조건 판정 |
| `feature::Feature` | 기능별 | 상태 없는 컨트롤러의 한 기능 — 받는 이벤트와 내는 액션을 선언 |
| `State` | 없음 (정적) | 한 상태의 태그와 진입/이탈 액션 |
| `Edge` | 없음 (정적) | 표의 한 줄 |
| `Machine` | 현재 태그 | 실행기 |

## 빠른 시작

가장 작은 예제로 훑어본다. 전등을 껐다 켰다 하는 2상태 FSM이다.

```rust
use fsm::machine::{Cond, Edge, Goto, Ignore, Machine, OnUnknown, Source, State};
use fsm::{Domain, StateDomain};

// 1. Domain — 컨트롤러가 쓸 타입들을 한 곳에 묶는다.
//    tags!/events! 가 enum과 커버리지용 전수 목록을 함께 만든다.
fsm::tags! {
    enum Tag { Off, On }
}

fsm::events! {
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

    // perform 이 유일한 필수 메서드다.
    // kind / all_tags / all_kinds 는 tags!/events! 가 만든 구현으로 해결된다.
    fn perform(action: Action, _ev: &Event, _world: &mut Env) {
        match action {
            Action::TurnOn => println!("on"),
            Action::TurnOff => println!("off"),
        }
    }
}

// 상태가 있으므로 StateDomain 도 구현한다.
impl StateDomain for Light {
    type Tag = Tag;
}

// 2. 상태 — 표의 또 다른 한 장. 진입/이탈 액션을 정적 데이터로 적는다.
static STATES: &[State<Light>] = &[
    State { tag: Tag::Off, entry: &[Action::TurnOff], exit: &[] },
    State { tag: Tag::On,  entry: &[Action::TurnOn],  exit: &[] },
];

// 3. 표 — 전이는 여기 한 곳에만 있다.
static EDGES: &[Edge<Light>] = &[
    Edge {
        id: "TURN_ON",
        from: Source::These(&[Tag::Off]),
        when: Kind::Toggle,
        check: fsm::check!(),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(Tag::On),
    },
    Edge {
        id: "TURN_OFF",
        from: Source::These(&[Tag::On]),
        when: Kind::Toggle,
        check: fsm::check!(),
        unknown: OnUnknown::Deny,
        run: &[],
        goto: Goto::To(Tag::Off),
    },
];

static IGNORES: &[Ignore<Light>] = &[];

fn main() {
    let mut world = Env;
    let mut m = Machine::new(Tag::Off, STATES, EDGES, IGNORES);

    m.dispatch(&Event::Toggle, &mut world); // -> on, state = On
    m.dispatch(&Event::Toggle, &mut world); // -> off, state = Off
}
```

더 복잡한 예제(상태 4개, 엣지 7개, 조건 조합, `Ignore`의 와일드카드 사용)는
[`src/main.rs`](src/main.rs)의 도어락 데모를 참고. `cargo run`으로 실행하면
[`example/door_lock.md`](example/door_lock.md)에 mermaid 다이어그램이 생성된다.

## 상세 사용법

### 1. `Domain` — 타입 묶음

프레임워크 전체가 제네릭 파라미터 `D: Domain` 하나만 받도록, 컨트롤러가 쓸
타입들을 `Domain` 트레이트 하나에 연관 타입으로 묶는다.

```rust
pub trait Domain: Sized + 'static {
    // 이벤트 본체 (payload 포함). Debug 는 dispatch 로그가 payload 를 담기 위해 필요하다.
    type Event: HasKind<Kind = Self::EventKind> + Debug;
    type EventKind: Enumerable;                  // 이벤트 종류 — 엣지가 이걸로 매칭
    type Action: Copy + Debug + 'static; // 동작 — 데이터로 표현해야 이름이 산출물에 찍힘
    type Env: ?Sized;                    // 바깥 세상 (api + DB)

    // 유일한 필수 메서드.
    fn perform(action: Self::Action, ev: &Self::Event, world: &mut Self::Env);

    // 커버리지 검사용 전수 목록. Enumerable::ALL 을 쓰는 기본 구현이 있으므로
    // 일부만 검사하고 싶을 때만 재정의한다.
    fn all_kinds() -> &'static [Self::EventKind] { ... }
}

// 상태가 있는 컨트롤러만 추가로 구현한다.
pub trait StateDomain: Domain {
    type Tag: Enumerable;                        // 상태 식별자
    fn all_tags() -> &'static [Self::Tag] { ... }
}
```

이벤트→종류 매핑은 `Domain`이 아니라 `HasKind`에만 있다. 매핑이 두 곳에 있으면
서로 어긋날 수 있으므로 진입점을 하나로 뒀다.

`Tag`와 `EventKind`는 `Enumerable`(= `Copy + Eq + Debug + 'static` + `const ALL`)을
요구한다. 커버리지 검사가 `(상태 × 이벤트)`를 전수 순회하려면 두 축의 값 목록이
필요한데, 그 목록을 손으로 관리하면 **변형을 추가하고 목록에 넣는 걸 깜빡했을 때
그 조합이 조용히 검사에서 빠진다** — 구멍을 잡는 장치 자체에 구멍이 생긴다.
그래서 enum과 목록을 한 선언에서 만드는 매크로를 쓴다.

```rust
fsm::tags! {
    enum Tag { Locked, Unlocked, Alarm, Maintenance }
}

fsm::events! {
    #[derive(Clone, Debug)]
    enum Event => Kind {
        EnterCode(u32),   // payload 있는 변형
        Timeout,          // 없는 변형 — 섞어 써도 된다
        Reset,
        MaintenanceToggle,
    }
}
```

`events!` 하나가 네 가지를 만든다. 손으로 쓰면 이 넷을 동기화해야 한다.

| 생성물 | 용도 |
|---|---|
| `enum Event` | payload를 가진 본체. 속성(`#[derive(..)]` 등)은 그대로 전달된다 |
| `enum Kind` | payload 없는 태그. `Copy + Eq + Debug` 자동 derive |
| `impl HasKind for Event` | `kind()` 기본 구현이 사용 |
| `impl Enumerable for Kind` | `all_kinds()` 기본 구현이 사용 |

그래서 `Domain` 구현에 남는 것은 **연관 타입 5개와 `perform` 하나**다.

여러 이벤트 변형을 한 종류로 묶고 싶으면(N:1 매핑) 매크로 대신 `HasKind`를 직접
구현한다. `Event`가 외부 크레이트 타입이라 고아 규칙에 걸리면 newtype으로 감싼다.

`perform`은 **`Env`를 변경할 수 있는 유일한 지점이다** — 조건 노드는 `&Env`만
받으므로 읽기만 한다. 그래서 이 컨트롤러가 바깥 세상에 하는
모든 변경은 `Action` 값을 거치고, 실행된 액션은 `Taken::actions`에 값 그대로
남는다. `perform`은 `Machine`에 접근할 수 없어 전이를 유발할 수는 없다.

`ev`는 처리 중인 이벤트다. `Edge::run`은 `&'static`이라 컴파일 타임 상수만
담을 수 있으므로, 런타임 값이 필요한 액션은 여기서 `ev`에서 꺼낸다.
진입/이탈 액션 목록도 `&'static`이라 마찬가지다. 상태에 종속된 값은 `Env`에
두고, 그 상태의 `entry` 액션이 `ev`에서 꺼내 쓰고 `exit` 액션이 지우면 값의
수명 전체가 표와 다이어그램에 드러난다.

```rust
// 표에는 상수만 (다이어그램에 이름이 찍힌다)
run: &[Action::RecordPosition],

// 값은 perform 에서 이벤트로부터 꺼낸다
Action::RecordPosition => {
    if let Event::PositionChanged(p) = ev { world.record_position(*p); }
}
```

### 2. 상태 — `State`

상태는 `Edge`와 마찬가지로 **정적 데이터**다. 태그와 진입/이탈 액션만 담고
행위는 갖지 않는다. 전이 로직은 물론 넣지 않는다 — 전이는 `Edge` 표에만 있어야
표 하나로 전체 구조를 파악할 수 있다.

```rust
static STATES: &[State<Domain타입>] = &[
    State { tag: Tag::Variant, entry: &[Action::A, Action::B], exit: &[Action::C] },
];
```

상태에서만 쓰는 변수도 `Env`에 둔다. `entry` 액션으로 초기화하고 `exit` 액션으로
지우면 "이 값은 이 상태에서만 유효하다"가 다이어그램에 그대로 그려진다 —
상태 안에 숨겨두면 그 수명이 코드에만 남는다.

### 3. 조건 — `Cond` (3치 논리) / `CondNode` / `check!`

조건 판정 결과는 `bool`이 아니라 3치(`Cond::True` / `False` / `Unknown`)다.
`Unknown`은 "api 조회 실패 등으로 판정 불가"를 뜻한다. `bool`로 뭉개면
실패했을 때의 정책이 조건 노드 안에 숨어버리므로, 3치로 두고 정책을
`Edge::unknown`에 명시해 다이어그램에도 드러나게 한다.

```rust
fsm::cond_node!(Domain타입, CondName, |cx| match cx.event {
    Event::Something(v) => Cond::from(*v == 기대값),
    _ => Cond::False,
});
```

- 조건 노드는 **내부 상태를 가질 수 없다** (`eval`이 `&self`). 필요한 입력은
  전부 `Cx`(이벤트 `cx.event`, 바깥 세상 `cx.world`)로 주입된다. 그래야 순수 함수로 테스트할 수 있고, 같은 dispatch 안에서
  여러 엣지가 같은 노드를 평가해도 `Memo` 캐시로 한 번만 계산된다.
- 조건 노드 이름(`name()`)은 머신 안에서 **유일해야 한다** — Memo 캐시 키이자
  다이어그램에 찍히는 이름이기 때문이다. `render::coverage`가 이름은 같은데
  타입이 다른 노드가 있는지 검사해 준다.

여러 조건을 조합할 때는 `check!` 매크로를 쓴다. `&&` 체인과 선행 `!`만
지원한다 (`||`가 필요하면 `Expr::Or`를 직접 구성한다):

```rust
check!()                        // 조건 없음, 항상 참
check!(A)                       // A
check!(!A)                      // !A
check!(A && B)                  // A && B
check!(!A && B && !C)           // !A && B && !C
```

### 4. 표 — `Edge` / `Source` / `Goto` / `OnUnknown`

```rust
pub struct Edge<D: Domain> {
    pub id: &'static str,          // 안정 식별자 — 순서가 바뀌어도 살아남아야 함
    pub from: Source<D>,           // 출발 상태 (조건이 아니라 상태 목록)
    pub when: D::EventKind,           // 매칭할 이벤트 종류
    pub check: &'static Expr<D>,   // check!() 로 만든 조건식
    pub unknown: OnUnknown,        // 조건이 Unknown일 때의 정책
    pub run: &'static [D::Action], // 이 전이에서만 실행할 액션 (선언 순서대로)
    pub goto: Goto<D>,             // 목표 상태
}
```

- **`Source<D>`**: `These(&[Tag, ...])`(나열한 상태만), `AnyExcept(&[Tag, ...])`
  (나열한 것만 제외한 모든 상태), `Any`(모든 상태). `AnyExcept`/`Any`는 계층형
  FSM에서 "이 이벤트는 어디서든 받는다" 같은 규칙을 한 줄로 표현해 DRY하게
  만든다.
- **`Goto<D>`**: `To(Tag)`(다른 상태로 전이, 이탈/진입 액션 실행) 또는
  `Internal`(상태는 그대로 두고 `run`만 실행 — 이탈/진입 액션은
  **돌지 않는다**).
- **`OnUnknown`**: `Deny`(모르면 전이하지 않음, fail-closed) 또는
  `Allow`(모르면 전이함).
- 같은 `(상태, 이벤트)`에 엣지가 여러 개 걸리면 **표에 선언한 순서가
  우선순위**다.

의도적으로 처리하지 않는 `(상태, 이벤트)` 조합은 `Ignore`로 명시한다. 이유를
반드시 적어야 하고, 이게 있어야 커버리지 검사에서 "빠뜨림"과 "의도"를
구분할 수 있다.

```rust
pub struct Ignore<D: Domain> {
    pub from: Source<D>,
    pub when: &'static [D::EventKind],
    pub why: &'static str,
}
```

### 5. 실행 — `Machine`

```rust
let mut m = Machine::new(초기_태그, STATES, EDGES, IGNORES);

m.dispatch(&event, &mut world); // -> Option<Taken>
```

`dispatch` 실행 순서는 `현재 상태의 exit` → 상태 전이 → `run` →
`목표 상태의 entry`다. 각 액션은 도달하는 즉시 수행되므로, 이탈 액션은 뒤따르는
효과가 반영되기 전의 세상을 본다. `Goto::Internal`이면 `run`만 실행된다. 채택된 엣지가
없으면 `None`을 반환하고, `Ignore`에도 안 걸리면 `log::warn!`을 남긴다
(로그를 보려면 `env_logger` 등 로거를 초기화해야 한다).

`Machine`의 가변 상태는 **현재 태그뿐이고 이벤트 큐가 없다.**
재진입은 큐가 아니라 borrow checker가 막는다 — `dispatch`가 도는 동안 `m`이
`&mut`로 대출되어 중첩 호출이 컴파일되지 않고, `Domain::perform`은 `Machine`에
접근할 방법이 없다.

후속 이벤트가 필요하면(예: 한 전이가 다른 전이를 유발) 호출부가 큐를 갖는다.
그러면 전이마다 `Taken`을 확인하면서 다음 이벤트를 결정할 수 있다.

```rust
let mut pending = VecDeque::from([첫_이벤트]);
while let Some(ev) = pending.pop_front() {
    if let Some(taken) = m.dispatch(&ev, &mut world) {
        // taken.edge / taken.actions 를 보고 후속 이벤트를 pending에 넣는다
    }
}
```

### 6. 산출물 — `render`

같은 표(`EDGES`, `IGNORES`)에서 세 가지를 뽑는다.

```rust
use fsm::render;

// mermaid stateDiagram-v2 문자열. Goto::Internal 엣지는 자기 자신으로 가는
// 화살표가 되어 다이어그램을 어지럽히므로 기본적으로 생략된다.
let diagram = render::to_mermaid::<MyDomain>(초기_태그, EDGES);

// 상태를 바꾸지 않는 전이(Internal)만 모은 마크다운 표.
let internal = render::internal_table::<MyDomain>(EDGES);

// (상태 × 이벤트) 전수 검사.
let cov = render::coverage::<MyDomain>(초기_태그, EDGES, IGNORES);
assert!(cov.is_clean()); // holes / unreachable / duplicate_node_names 가 모두 비어야 함
```

`Coverage`가 담는 정보:

| 필드 | 의미 |
|---|---|
| `holes` | 엣지도 `Ignore`도 없는 `(상태, 이벤트)` 조합. **CI에서 0이어야 한다.** |
| `overlaps` | 같은 조합에 엣지가 2개 이상 걸린 경우 (오류는 아니지만 리뷰 대상) |
| `unreachable` | 초기 상태에서 엣지를 따라가도 도달할 수 없는 상태 |
| `duplicate_node_names` | 이름은 같은데 타입이 다른 조건 노드 (Memo 캐시 오염 위험) |

CI/테스트에 `cov.is_clean()`을 assert 해 두면, 새 상태나 이벤트를 추가하고
처리를 깜빡했을 때 컴파일이 아니라 테스트에서 바로 잡힌다.

### PlantUML로 변환

mermaid `stateDiagram-v2`와 PlantUML 상태도는 문법이 거의 같다 — `[*] --> X`와
`A --> B`가 그대로 통하고 헤더·레이블 구분자·줄바꿈만 다르다. 그래서 다이어그램
생성기를 하나 더 두는 대신 변환 스크립트를 쓴다.

```sh
scripts/mermaid_to_plantuml.sh example/door_lock.md > example/door_lock.puml

# 이미지로 바로 뽑을 때
scripts/mermaid_to_plantuml.sh example/door_lock.md | plantuml -p > fsm.png
```

마크다운 파일(```mermaid 펜스 포함)과 생 다이어그램 모두 받고, 인자가 없으면
표준 입력을 읽는다. 변환 내용은 세 줄뿐이다.

| mermaid | PlantUML |
|---|---|
| `stateDiagram-v2` | `@startuml` + `hide empty description` … `@enduml` |
| `A --> B: label` | `A --> B : label` |
| `<br/>` | `\n` |

## 프로젝트 구조

```
src/
  lib.rs          // Domain 트레이트 + 모듈 재노출 — 라이브러리 진입점
  main.rs         // 사용 예제 (도어락 데모, cargo run으로 실행)
  cond.rs         // Cond (3치 논리)
  enums.rs        // Enumerable, tags!/events! 매크로
  node.rs         // CondNode, Cx, Expr, Memo, cond_node!/check! 매크로
  state.rs        // State (태그 + 진입/이탈 액션)
  edge.rs         // Edge, Source, Goto, OnUnknown, Ignore
  machine.rs      // Machine (실행기)
  render.rs       // to_mermaid, internal_table, coverage
  tests.rs        // 프레임워크 자체 테스트 (후방 카메라 예제)
example/
  door_lock.md    // cargo run 시 생성되는 mermaid 다이어그램
```

## 테스트

```sh
cargo test
```

`src/tests.rs`가 후방 카메라 컨트롤러를 예제로 프레임워크 전체 기능
(진입/이탈 액션, `Goto::Internal`, `OnUnknown::Deny`/`Allow`, 선언 순서
우선순위, `Ignore`, mermaid 골든 테스트, 커버리지 검사)을 검증한다.
