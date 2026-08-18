//! Three-valued logic for guards. `Unknown` means undecidable (e.g. a failed
//! lookup), kept apart from `False` so the fallback policy is explicit in
//! [`crate::machine::OnUnknown`] instead of hidden inside a guard.

/// The result of evaluating one guard.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Cond {
    True,
    False,
    /// Undecidable, e.g. a failed lookup.
    Unknown,
}

impl From<bool> for Cond {
    /// Maps `true`/`false` to [`Cond::True`]/[`Cond::False`]. There is no
    /// `bool` counterpart for [`Cond::Unknown`] — construct it directly.
    fn from(b: bool) -> Self {
        if b { Self::True } else { Self::False }
    }
}

impl Cond {
    /// Kleene AND: a single `False` wins over any `Unknown`.
    pub const fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::False, _) | (_, Self::False) => Self::False,
            (Self::True, Self::True) => Self::True,
            _ => Self::Unknown,
        }
    }

    /// Kleene OR: a single `True` wins over any `Unknown`.
    pub const fn or(self, other: Self) -> Self {
        match (self, other) {
            (Self::True, _) | (_, Self::True) => Self::True,
            (Self::False, Self::False) => Self::False,
            _ => Self::Unknown,
        }
    }

    /// Kleene NOT: `Unknown` negates to `Unknown`.
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
