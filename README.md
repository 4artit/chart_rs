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
| `StateNode` | 있음 | 상태 + 그 상태에서만 사는 변수 |
| `Edge` | 없음 (정적) | 표의 한 줄 |
| `Machine` | 현재 태그 + 상태 노드들 | 실행기 |

## 빠른 시작

가장 작은 예제로 훑어본다. 전등을 껐다 켰다 하는 2상태 FSM이다.

```rust
use fsm::{Cond, Domain, Edge, Goto, Ignore, Machine, OnUnknown, Source, StateNode};

// 1. Domain — 컨트롤러가 쓸 타입들을 한 곳에 묶는다.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Tag { Off, On }

#[derive(Clone, Debug)]
enum Event { Toggle }

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Kind { Toggle }

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Action { TurnOn, TurnOff }

struct Env;
struct Light;

impl Domain for Light {
    type Tag = Tag;
    type Event = Event;
    type EventKind = Kind;
    type Action = Action;
    type Env = Env;

    fn kind(ev: &Event) -> Kind {
        match ev { Event::Toggle => Kind::Toggle }
    }

    fn perform(action: Action, _state: &dyn StateNode<Self>, _world: &mut Env) {
        match action {
            Action::TurnOn => println!("on"),
            Action::TurnOff => println!("off"),
        }
    }

    fn all_tags() -> &'static [Tag] { &[Tag::Off, Tag::On] }
    fn all_kinds() -> &'static [Kind] { &[Kind::Toggle] }
}

// 2. 상태 — 진입/이탈 액션은 state! 매크로로 선언한다.
fsm::state!(Light, Off, tag: Tag::Off, on_enter: [Action::TurnOff]);
fsm::state!(Light, On, tag: Tag::On, on_enter: [Action::TurnOn]);

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
    let mut m = Machine::new(Tag::Off, vec![Box::new(Off), Box::new(On)], EDGES, IGNORES);

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
    type Tag: Copy + Eq + Debug + 'static;   // 상태 식별자 (값은 StateNode가 가짐)
    type Event;                                  // 이벤트 본체 (payload 포함)
    type EventKind: Copy + Eq + Debug + 'static; // 이벤트 종류 — 엣지가 이걸로 매칭
    type Action: Copy + Debug + 'static;      // 동작 — 데이터로 표현해야 이름이 산출물에 찍힘
    type Env: ?Sized;                       // 바깥 세상 (api + DB)

    fn kind(ev: &Self::Event) -> Self::EventKind;
    fn perform(action: Self::Action, state: &dyn StateNode<Self>, world: &mut Self::Env);
    fn all_tags() -> &'static [Self::Tag];    // 커버리지 검사용 전체 상태 목록
    fn all_kinds() -> &'static [Self::EventKind]; // 커버리지 검사용 전체 이벤트 종류 목록
}
```

`perform` 안에서 `Machine::dispatch`를 재호출하면 안 된다 — 재진입을 막기
위해 이벤트를 발생시키려면 액션 실행 이후 `Machine::post`로 큐에 넣는다.

### 2. 상태 — `StateNode` / `state!`

상태는 태그뿐 아니라 **그 상태에서만 사는 변수**를 가질 수 있다. 단, 전이
로직은 절대 넣지 않는다 — 전이는 `Edge` 표에만 있어야 표 하나로 전체 구조를
파악할 수 있다.

```rust
fsm::state!(Domain타입, StructName, tag: Tag::Variant);
fsm::state!(Domain타입, StructName, tag: Tag::Variant, on_enter: [Action::A, Action::B]);
fsm::state!(Domain타입, StructName, tag: Tag::Variant, on_exit: [Action::C]);
fsm::state!(Domain타입, StructName, tag: Tag::Variant,
            on_enter: [Action::A], on_exit: [Action::C]);
```

변수가 있는 상태가 필요하면 매크로 대신 `StateNode`를 직접 구현한다.
`on_exit`에서 상태 한정 변수를 초기화하는 것이 "이 변수는 이 상태에서만
유효하다"는 규약을 실제로 보장한다.

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
  전부 `Cx`(이벤트 `cx.event`, 바깥 세상 `cx.world`, 현재 상태 `cx.state`)로
  주입된다. 그래야 순수 함수로 테스트할 수 있고, 같은 dispatch 안에서
  여러 엣지가 같은 노드를 평가해도 `Memo` 캐시로 한 번만 계산된다.
- `cx.state_as::<S>()`로 현재 상태 노드를 구체 타입으로 다운캐스트할 수
  있다 (태그로 분기하지 말고 상태 변수를 직접 읽으라는 뜻).
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
- **`Goto<D>`**: `To(Tag)`(다른 상태로 전이, `on_exit`/`on_enter` 실행) 또는
  `Internal`(상태는 그대로 두고 `run`만 실행 — `on_exit`/`on_enter`는
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
let mut m = Machine::new(초기_태그, vec![Box::new(State1), Box::new(State2), ...], EDGES, IGNORES);

m.dispatch(&event, &mut world); // -> Option<Taken>
```

`dispatch` 실행 순서는 `on_exit(현재 상태)` → `run` → 상태 전이 →
`on_enter(목표 상태)`다. `Goto::Internal`이면 `run`만 실행된다. 채택된 엣지가
없으면 `None`을 반환하고, `Ignore`에도 안 걸리면 `log::warn!`을 남긴다
(로그를 보려면 `env_logger` 등 로거를 초기화해야 한다).

액션이 새 이벤트를 만들어야 하면(예: 다른 전이를 트리거) `perform` 안에서
바로 `dispatch`를 다시 부르지 말고 `Machine::post`로 큐에 넣는다. 큐는
`Machine::pump`로 한꺼번에 소진한다 — 이렇게 하면 전이 하나가 완전히 끝난
뒤에만 다음 이벤트가 처리되어 재진입이 구조적으로 불가능해진다.

```rust
m.post(다른_이벤트);
m.pump(&mut world);
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

## 프로젝트 구조

```
src/
  lib.rs          // Domain 트레이트 + 모듈 재노출 — 라이브러리 진입점
  main.rs         // 사용 예제 (도어락 데모, cargo run으로 실행)
  cond.rs         // Cond (3치 논리)
  node.rs         // CondNode, Cx, Expr, Memo, cond_node!/check! 매크로
  state.rs        // StateNode, state! 매크로
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
