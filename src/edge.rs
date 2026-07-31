//! 표의 한 줄.

use super::{Domain, Expr};

/// 출발 상태 지정. **조건이 아니라 상태 목록이다.**
pub enum Source<D: Domain> {
    These(&'static [D::Tag]),
    /// 나열한 것을 뺀 모든 상태. 계층형 FSM의 DRY 이점만 가져온다.
    AnyExcept(&'static [D::Tag]),
    Any,
}

impl<D: Domain> Source<D> {
    pub fn matches(&self, tag: D::Tag) -> bool {
        match self {
            Self::These(list) => list.contains(&tag),
            Self::AnyExcept(list) => !list.contains(&tag),
            Self::Any => true,
        }
    }

    /// 다이어그램·커버리지용 구체 상태 목록. 와일드카드는 여기서 전개된다.
    pub fn expand(&self) -> Vec<D::Tag> {
        D::all_tags()
            .iter()
            .copied()
            .filter(|t| self.matches(*t))
            .collect()
    }
}

/// 목표 상태.
pub enum Goto<D: Domain> {
    To(D::Tag),
    /// 상태 불변. `on_exit`/`on_enter`가 **돌지 않는다.**
    Internal,
}

/// 조건이 [`super::Cond::Unknown`]일 때의 정책.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum OnUnknown {
    /// 모르면 전이하지 않는다 (fail-closed). 현행 미러 컨트롤러의 기본값.
    Deny,
    /// 모르면 전이한다.
    Allow,
}

/// 전이 한 줄.
///
/// 같은 `(상태, 이벤트)`에 여러 엣지가 걸리면 **표에 선언한 순서가 우선순위**다.
pub struct Edge<D: Domain> {
    /// 안정 식별자. 요구사항 추적·골든 diff용. 순서가 바뀌어도 살아남아야 한다.
    pub id: &'static str,
    pub from: Source<D>,
    pub when: D::EventKind,
    pub check: &'static Expr<D>,
    pub unknown: OnUnknown,
    /// 이 전이에서만 수행하는 액션. 순서 그대로 실행된다.
    pub run: &'static [D::Action],
    pub goto: Goto<D>,
}

/// 의도적으로 처리하지 않는 조합. **이유를 반드시 적는다.**
///
/// 이게 있어야 커버리지 검사에서 "구멍"과 "의도"를 구분할 수 있다.
pub struct Ignore<D: Domain> {
    pub from: Source<D>,
    pub when: &'static [D::EventKind],
    pub why: &'static str,
}

impl<D: Domain> Ignore<D> {
    pub fn matches(&self, tag: D::Tag, kind: D::EventKind) -> bool {
        self.from.matches(tag) && self.when.contains(&kind)
    }
}
