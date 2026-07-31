//! 3치 논리. `Unknown`은 "api 조회가 실패해서 모른다"를 뜻한다.
//!
//! `bool`로 뭉개면 실패 정책이 조건 노드 안에 숨는다. 3치로 두면
//! 정책이 [`crate::fsm::Edge::unknown`]에 명시되어 다이어그램에 나온다.

/// 조건 평가 결과.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Cond {
    True,
    False,
    /// 조회 실패 등으로 판정 불가.
    Unknown,
}

impl From<bool> for Cond {
    fn from(b: bool) -> Self {
        if b {
            Self::True
        } else {
            Self::False
        }
    }
}

impl Cond {
    /// Kleene AND. `False`가 하나라도 있으면 `False` — 이게 fail-safe 조합을 만든다.
    pub const fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::False, _) | (_, Self::False) => Self::False,
            (Self::True, Self::True) => Self::True,
            _ => Self::Unknown,
        }
    }

    /// Kleene OR.
    pub const fn or(self, other: Self) -> Self {
        match (self, other) {
            (Self::True, _) | (_, Self::True) => Self::True,
            (Self::False, Self::False) => Self::False,
            _ => Self::Unknown,
        }
    }

    /// Kleene NOT. `Unknown`의 부정은 `Unknown`이다.
    pub const fn not(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Unknown => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Cond::{False, True, Unknown};

    #[test]
    fn and_is_false_dominant() {
        assert_eq!(False.and(Unknown), False);
        assert_eq!(Unknown.and(False), False);
        assert_eq!(True.and(Unknown), Unknown);
        assert_eq!(True.and(True), True);
    }

    #[test]
    fn or_is_true_dominant() {
        assert_eq!(True.or(Unknown), True);
        assert_eq!(Unknown.or(True), True);
        assert_eq!(False.or(Unknown), Unknown);
        assert_eq!(False.or(False), False);
    }

    #[test]
    fn not_preserves_unknown() {
        assert_eq!(Unknown.not(), Unknown);
        assert_eq!(True.not(), False);
        assert_eq!(False.not(), True);
    }
}
