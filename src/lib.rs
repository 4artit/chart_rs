//! 선언형 FSM 프레임워크.
//!
//! 컨트롤러의 전이를 `&'static [Edge<D>]` 표 하나로 선언하면
//! 실행·다이어그램 생성·전수 커버리지 검사를 모두 같은 표에서 얻는다.
//!
//! 자세한 사용법은 저장소 루트의 `README.md` 참고.
//!
//! # 구성 요소
//!
//! | 요소 | 내부 상태 | 역할 |
//! |---|---|---|
//! | [`Domain`] | — | 컨트롤러가 쓸 타입 묶음 (Tag / Event / Action / Env) |
//! | [`CondNode`] | **없음 (`&self`)** | 전이 조건 판정 |
//! | [`StateNode`] | **있음** | 상태 + 그 상태에서만 사는 변수 |
//! | [`Edge`] | 없음 (정적) | 표의 한 줄 |
//! | [`Machine`] | 현재 태그 + 상태 노드들 | 실행기 |

mod cond;
mod edge;
mod machine;
mod node;
pub mod render;
mod state;

#[cfg(test)]
mod tests;

pub use cond::Cond;
pub use edge::{Edge, Goto, Ignore, OnUnknown, Source};
pub use machine::Machine;
pub use node::{CondNode, Cx, Expr, Memo};
pub use state::StateNode;

use std::fmt::Debug;

/// 컨트롤러 하나가 쓸 타입 묶음.
///
/// 제네릭 파라미터를 `D` 하나로 묶어 프레임워크 전체가 단일 타입 인자만 갖게 한다.
pub trait Domain: Sized + 'static {
    /// 상태 식별자. 값(payload)은 [`StateNode`]가 갖고, 태그만 여기 온다.
    type Tag: Copy + Eq + Debug + 'static;
    /// 이벤트 본체 (payload 포함).
    type Event;
    /// 이벤트 종류. payload 없는 태그 — 엣지가 이걸로 매칭한다.
    type EventKind: Copy + Eq + Debug + 'static;
    /// 동작. **데이터로 표현**해야 이름이 산출물에 찍힌다.
    type Action: Copy + Debug + 'static;
    /// 바깥 세상 (api + DB).
    type Env: ?Sized;

    /// 이벤트에서 종류 태그를 뽑는다.
    fn kind(ev: &Self::Event) -> Self::EventKind;

    /// 액션을 실제로 수행한다. 여기서 [`Machine::dispatch`]를 재호출하면 안 된다.
    fn perform(action: Self::Action, state: &dyn StateNode<Self>, world: &mut Self::Env);

    /// 커버리지 검사를 위한 전체 상태 목록.
    fn all_tags() -> &'static [Self::Tag];

    /// 커버리지 검사를 위한 전체 이벤트 종류 목록.
    fn all_kinds() -> &'static [Self::EventKind];
}
