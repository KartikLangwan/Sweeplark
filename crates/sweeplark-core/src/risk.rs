use serde::{Deserialize, Serialize};

/// How risky a single action is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    /// Cannot hurt the user's data or break Windows (caches, temp files). May run automatically.
    Safe,
    /// Usually fine but has a visible effect (logs out of sites, app starts slower). Always asks.
    Ask,
    /// Only for people who know what they are doing. Hidden unless Expert mode is on, always asks.
    Expert,
}

impl Risk {
    /// Only safe work may be applied without a confirmation from the user.
    pub fn may_auto_apply(self) -> bool {
        self == Risk::Safe
    }
}

/// The user's chosen trust level: which risk levels are shown at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    /// Only safe items are shown.
    Safe,
    /// Safe and ask items are shown (default).
    #[default]
    Ask,
    /// Everything is shown.
    Expert,
}

impl TrustLevel {
    pub fn max_risk(self) -> Risk {
        match self {
            TrustLevel::Safe => Risk::Safe,
            TrustLevel::Ask => Risk::Ask,
            TrustLevel::Expert => Risk::Expert,
        }
    }

    pub fn shows(self, risk: Risk) -> bool {
        risk <= self.max_risk()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_safe_auto_applies() {
        assert!(Risk::Safe.may_auto_apply());
        assert!(!Risk::Ask.may_auto_apply());
        assert!(!Risk::Expert.may_auto_apply());
    }

    #[test]
    fn trust_levels_filter() {
        assert!(TrustLevel::Safe.shows(Risk::Safe));
        assert!(!TrustLevel::Safe.shows(Risk::Ask));
        assert!(TrustLevel::Ask.shows(Risk::Ask));
        assert!(!TrustLevel::Ask.shows(Risk::Expert));
        assert!(TrustLevel::Expert.shows(Risk::Expert));
    }
}
