//! A claim is one upstream's statement about one credential.
//!
//! The two sources disagree about what to record, so they stay two variants
//! rather than a lowest common denominator. `label_prefix` + `contract` is
//! Strica's displayed label; Eternl's is `project` + `contract`. The parts are
//! kept separate — a consumer joins them, the importer never does.
//!
//! `purpose` and `type` on an Eternl claim are the upstream's Plutus purpose
//! (`SPEND`, `MINT`), not the behaviour `kind` the authored layer assigns.

use serde::ser::{Serialize, SerializeStruct, Serializer};
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Strica,
    Eternl,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Strica => "strica",
            Source::Eternl => "eternl",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct Audit {
    pub provider: Option<String>,
    pub report: Option<String>,
    pub date: Option<String>,
}

impl Serialize for Audit {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut out = serializer.serialize_struct("audit", 3)?;
        out.serialize_field("provider", &self.provider)?;
        out.serialize_field("report", &self.report)?;
        out.serialize_field("date", &self.date)?;
        out.end()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct StricaClaim {
    pub project: String,
    pub label_prefix: String,
    pub contract: Option<String>,
    pub version: Option<i64>,
    pub language: Option<String>,
    pub language_version: Option<i64>,
    pub github: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub website: Option<String>,
    pub audits: Vec<Audit>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct EternlClaim {
    pub project: String,
    pub contract: Option<String>,
    pub purpose: Option<String>,
    #[serde(rename = "type")]
    pub script_type: Option<String>,
    pub plutus_version: Option<i64>,
    pub category: Option<String>,
    pub sub_category: Option<String>,
    pub website: Option<String>,
    pub github: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Claim {
    Strica(StricaClaim),
    Eternl(EternlClaim),
}

impl Claim {
    pub fn source(&self) -> Source {
        match self {
            Claim::Strica(_) => Source::Strica,
            Claim::Eternl(_) => Source::Eternl,
        }
    }

    pub fn project(&self) -> &str {
        match self {
            Claim::Strica(claim) => &claim.project,
            Claim::Eternl(claim) => &claim.project,
        }
    }

    pub fn contract(&self) -> Option<&str> {
        match self {
            Claim::Strica(claim) => claim.contract.as_deref(),
            Claim::Eternl(claim) => claim.contract.as_deref(),
        }
    }
}

impl Ord for Claim {
    fn cmp(&self, other: &Self) -> Ordering {
        self.source()
            .as_str()
            .cmp(other.source().as_str())
            .then_with(|| self.project().cmp(other.project()))
            .then_with(|| {
                self.contract()
                    .unwrap_or_default()
                    .cmp(other.contract().unwrap_or_default())
            })
    }
}

impl PartialOrd for Claim {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Reading back what this tool writes. The tag is the `source` field, so the two
/// upstream shapes stay two variants on the way in as well as out.
#[derive(serde::Deserialize)]
#[serde(tag = "source", rename_all = "lowercase")]
enum ClaimWire {
    Strica(StricaClaim),
    Eternl(EternlClaim),
}

impl<'de> serde::Deserialize<'de> for Claim {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        Ok(match ClaimWire::deserialize(deserializer)? {
            ClaimWire::Strica(claim) => Claim::Strica(claim),
            ClaimWire::Eternl(claim) => Claim::Eternl(claim),
        })
    }
}

impl Serialize for Claim {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Claim::Strica(claim) => {
                let mut out = serializer.serialize_struct("claim", 12)?;
                out.serialize_field("source", Source::Strica.as_str())?;
                out.serialize_field("project", &claim.project)?;
                out.serialize_field("label_prefix", &claim.label_prefix)?;
                out.serialize_field("contract", &claim.contract)?;
                out.serialize_field("version", &claim.version)?;
                out.serialize_field("language", &claim.language)?;
                out.serialize_field("language_version", &claim.language_version)?;
                out.serialize_field("github", &claim.github)?;
                out.serialize_field("description", &claim.description)?;
                out.serialize_field("category", &claim.category)?;
                out.serialize_field("website", &claim.website)?;
                out.serialize_field("audits", &claim.audits)?;
                out.end()
            }
            Claim::Eternl(claim) => {
                let mut out = serializer.serialize_struct("claim", 10)?;
                out.serialize_field("source", Source::Eternl.as_str())?;
                out.serialize_field("project", &claim.project)?;
                out.serialize_field("contract", &claim.contract)?;
                out.serialize_field("purpose", &claim.purpose)?;
                out.serialize_field("type", &claim.script_type)?;
                out.serialize_field("plutus_version", &claim.plutus_version)?;
                out.serialize_field("category", &claim.category)?;
                out.serialize_field("sub_category", &claim.sub_category)?;
                out.serialize_field("website", &claim.website)?;
                out.serialize_field("github", &claim.github)?;
                out.end()
            }
        }
    }
}

/// A credential is exactly 56 lowercase hex. Anything else is dropped and
/// reported, never coerced into the index.
pub fn credential(raw: &str) -> Option<String> {
    let lowered = raw.to_lowercase();
    let is_hex = lowered
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    (lowered.len() == 56 && is_hex).then_some(lowered)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strica(project: &str, contract: Option<&str>) -> Claim {
        Claim::Strica(StricaClaim {
            project: project.to_string(),
            label_prefix: project.to_string(),
            contract: contract.map(str::to_string),
            version: None,
            language: None,
            language_version: None,
            github: None,
            description: None,
            category: None,
            website: None,
            audits: Vec::new(),
        })
    }

    #[test]
    fn a_policy_id_and_its_asset_name_is_not_a_credential() {
        // TSURU: the 56-hex policy id followed by the 10-hex asset name.
        let policy_and_asset = "ac015c38917f306a84748c2d646bed90bdd64421c592163e60702d735453555255";
        assert_eq!(policy_and_asset.len(), 66);
        assert_eq!(credential(policy_and_asset), None);
    }

    #[test]
    fn a_policy_id_is_a_credential_by_shape() {
        // The same 56 hex is both a policy id and a payment credential; nothing
        // in the string tells them apart, which is why a policy id must never
        // reach a tool that expects an address.
        let policy = "ac015c38917f306a84748c2d646bed90bdd64421c592163e60702d73";
        assert_eq!(credential(policy).as_deref(), Some(policy));
    }

    #[test]
    fn a_credential_is_lowercased_and_kept() {
        let upper = "A65CA58A0F5C5C1D1B0A21B5EE5A62C1F2B4F77A6A4F7C6A8A2E1B3C";
        assert_eq!(
            credential(upper).as_deref(),
            Some("a65ca58a0f5c5c1d1b0a21b5ee5a62c1f2b4f77a6a4f7c6a8a2e1b3c")
        );
    }

    #[test]
    fn claims_sort_by_source_then_project_then_contract() {
        let mut claims = [
            strica("Zebra", Some("B")),
            strica("Alpha", None),
            strica("Alpha", Some("A")),
        ];
        claims.sort();
        let order: Vec<(&str, &str)> = claims
            .iter()
            .map(|claim| (claim.project(), claim.contract().unwrap_or("")))
            .collect();
        assert_eq!(order, vec![("Alpha", ""), ("Alpha", "A"), ("Zebra", "B")]);
    }
}
