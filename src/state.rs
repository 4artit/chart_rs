//! 상태 노드. 상태 태그 + **그 상태에서만 사는 변수**를 갖는다.

use std::any::Any;

use super::Domain;

/// 상태 하나.
///
/// 변수를 가질 수 있고 진입/이탈 동작을 정의할 수 있다. 다만 **전이 로직은
/// 여기 넣지 않는다** — 전이는 [`super::Edge`] 표에만 있어야 표 하나로 구조를
/// 파악할 수 있다.
///
/// `on_exit`에서 변수를 초기화하는 것이 "상태 한정"을 실제로 보장한다.
pub trait StateNode<D: Domain>: Any {
    fn tag(&self) -> D::Tag;

    /// 진입 시 수행할 액션을 `out`에 넣는다. 직접 세상을 바꾸지 않는다.
    fn on_enter(&mut self, _ev: &D::Event, _world: &D::Env, _out: &mut Vec<D::Action>) {}

    /// 이탈 시 수행할 액션을 `out`에 넣고, 상태 한정 변수를 초기화한다.
    fn on_exit(&mut self, _world: &D::Env, _out: &mut Vec<D::Action>) {}

    /// `Cx::state_as`를 위한 다운캐스트 훅. `state!` 매크로가 자동 생성한다.
    fn as_any(&self) -> &dyn Any;
}

/// 변수 없는 상태를 선언한다.
///
/// ```ignore
/// state!(RearCam, Off,     tag: Tag::Off);
/// state!(RearCam, Showing, tag: Tag::Showing,
///        on_enter: [Action::ShowCamera],
///        on_exit:  [Action::HideCamera]);
/// ```
#[macro_export]
macro_rules! state {
    ($dom:ty, $name:ident, tag: $tag:expr) => {
        $crate::state!($dom, $name, tag: $tag, on_enter: [], on_exit: []);
    };
    ($dom:ty, $name:ident, tag: $tag:expr, on_enter: [$($enter:expr),* $(,)?]) => {
        $crate::state!($dom, $name, tag: $tag, on_enter: [$($enter),*], on_exit: []);
    };
    ($dom:ty, $name:ident, tag: $tag:expr, on_exit: [$($exit:expr),* $(,)?]) => {
        $crate::state!($dom, $name, tag: $tag, on_enter: [], on_exit: [$($exit),*]);
    };
    ($dom:ty, $name:ident, tag: $tag:expr,
     on_enter: [$($enter:expr),* $(,)?], on_exit: [$($exit:expr),* $(,)?]) => {
        #[derive(Default)]
        pub struct $name;

        impl $crate::StateNode<$dom> for $name {
            fn tag(&self) -> <$dom as $crate::Domain>::Tag {
                $tag
            }
            fn on_enter(
                &mut self,
                _ev: &<$dom as $crate::Domain>::Event,
                _world: &<$dom as $crate::Domain>::Env,
                out: &mut Vec<<$dom as $crate::Domain>::Action>,
            ) {
                let _ = &out; // 액션 목록이 비어 있을 때의 unused 경고 억제
                $(out.push($enter);)*
            }
            fn on_exit(
                &mut self,
                _world: &<$dom as $crate::Domain>::Env,
                out: &mut Vec<<$dom as $crate::Domain>::Action>,
            ) {
                let _ = &out; // 액션 목록이 비어 있을 때의 unused 경고 억제
                $(out.push($exit);)*
            }
            fn as_any(&self) -> &dyn ::std::any::Any {
                self
            }
        }
    };
}
