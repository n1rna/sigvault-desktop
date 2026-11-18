//! `setup/policy.ts` — wallet policy model shared by the create and
//! recover flows.
//!
//! A policy is a set of spending paths; each path needs `threshold` of
//! its slots to sign, optionally after a relative timelock. Slots are
//! the key positions the user fills — by generating a key here, reading
//! one off a hardware wallet, typing a seed, or importing a key file.

use std::collections::{HashMap, HashSet};

use super::api::{
    AnalyzedSlot, DescriptorAnalysis, HotSecret, ScriptKind, SlotKey, SlotSource, WirePath,
    WirePolicy, WireRecovery,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Threshold {
    pub threshold: u32,
    pub n: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryDraft {
    pub timelock_blocks: u32,
    pub threshold: u32,
    pub n: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyDraft {
    Singlesig,
    Multisig {
        threshold: u32,
        n: u32,
    },
    Timelocked {
        script: ScriptKind,
        /// `None` = unspendable primary (recovery-only wallet). Taproot only.
        primary: Option<Threshold>,
        recoveries: Vec<RecoveryDraft>,
    },
}

pub struct PolicyPreset {
    pub id: &'static str,
    pub title: &'static str,
    pub hint: &'static str,
    pub shape: &'static str,
    pub draft: PolicyDraft,
}

/// ~144 blocks per day. Liana-style timelocks max out at 65535 blocks.
pub const BLOCKS_PER_DAY: u32 = 144;
pub const MAX_TIMELOCK_BLOCKS: u32 = 65535;

pub const TIMELOCK_PRESETS: [(&str, u32); 5] = [
    ("1 week", 1008),
    ("1 month", 4320),
    ("3 months", 12960),
    ("6 months", 25920),
    ("1 year", 52560),
];

pub fn default_timelocked() -> PolicyDraft {
    PolicyDraft::Timelocked {
        script: ScriptKind::Taproot,
        primary: Some(Threshold { threshold: 1, n: 1 }),
        recoveries: vec![RecoveryDraft {
            timelock_blocks: 25920,
            threshold: 1,
            n: 1,
        }],
    }
}

pub fn common_presets() -> Vec<PolicyPreset> {
    vec![
        PolicyPreset {
            id: "singlesig",
            title: "Single key",
            hint: "One key controls the wallet. The simplest setup.",
            shape: "wpkh · P2WPKH",
            draft: PolicyDraft::Singlesig,
        },
        PolicyPreset {
            id: "multisig-2of3",
            title: "2-of-3 multisig",
            hint: "Any two of three keys can spend. Losing one key is survivable.",
            shape: "wsh(sortedmulti) · P2WSH",
            draft: PolicyDraft::Multisig { threshold: 2, n: 3 },
        },
        PolicyPreset {
            id: "timelocked-recovery",
            title: "Key + timelocked recovery",
            hint: "Your key spends any time. A backup key can spend after 6 months without activity.",
            shape: "Miniscript · Taproot",
            draft: default_timelocked(),
        },
    ]
}

fn js_round(x: f64) -> i64 {
    (x + 0.5).floor() as i64
}

pub fn describe_timelock(blocks: u32) -> String {
    let days = blocks as f64 / BLOCKS_PER_DAY as f64;
    if days < 1.0 {
        return format!("{blocks} blocks");
    }
    if days < 60.0 {
        let d = js_round(days);
        return format!("~{d} day{}", if d == 1 { "" } else { "s" });
    }
    let months = days / 30.0;
    if months < 18.0 {
        return format!("~{} months", js_round(months));
    }
    format!("~{:.1} years", days / 365.0)
}

/// BIP44 coin index — mainnet 0', every test network 1'.
fn coin(network: &str) -> &'static str {
    if network == "bitcoin" {
        "0'"
    } else {
        "1'"
    }
}

/// Standard account path for a key in this kind of policy: BIP84 for
/// single-sig, BIP48 (script type 2' = P2WSH) for anything multi-key.
/// `account` lets two slots share a seed without sharing a key.
pub fn default_derivation_path(policy: &PolicyDraft, network: &str, account: u32) -> String {
    match policy {
        PolicyDraft::Singlesig => format!("m/84'/{}/{account}'", coin(network)),
        _ => format!("m/48'/{}/{account}'/2'", coin(network)),
    }
}

/// Bump the account component of a standard path. Returns `None` for
/// paths that don't follow BIP84/BIP48 — those the user picked by hand.
#[allow(dead_code)] // setup/slot_editor.rs
pub fn with_account(path: &str, account: u32) -> Option<String> {
    let parts: Vec<&str> = path.split('/').collect();
    if !(parts.len() == 4 || parts.len() == 5) || parts[0] != "m" {
        return None;
    }
    if parts[1] != "84'" && parts[1] != "48'" {
        return None;
    }
    if parts[2] != "0'" && parts[2] != "1'" {
        return None;
    }
    let acct = parts[3].strip_suffix('\'')?;
    if acct.is_empty() || !acct.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let tail = if parts.len() == 5 {
        if parts[4] != "2'" {
            return None;
        }
        "/2'"
    } else {
        ""
    };
    Some(format!("m/{}/{}/{account}'{tail}", parts[1], parts[2]))
}

#[derive(Clone, Debug, PartialEq)]
pub struct PathSpec {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub threshold: u32,
    pub timelock_blocks: Option<u32>,
    pub slot_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FilledKey {
    pub source: SlotSource,
    pub key: SlotKey,
    pub secret: Option<HotSecret>,
    /// Hardware model, key-file name, etc. Display only.
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub id: String,
    pub label: String,
    pub path_id: String,
    /// Where a new key for this slot is derived.
    pub derivation_path: String,
    /// Recover mode: the key the descriptor already names. Locked — the
    /// user can only attach the matching private key to it.
    pub expected: Option<AnalyzedSlot>,
    pub filled: Option<FilledKey>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct SlotLayout {
    pub paths: Vec<PathSpec>,
    pub slots: Vec<Slot>,
}

impl SlotLayout {
    pub fn slot(&self, id: &str) -> Option<&Slot> {
        self.slots.iter().find(|s| s.id == id)
    }
}

fn letter(i: usize) -> char {
    (b'A' + i as u8) as char
}

/// Lay out paths and empty slots for a policy draft.
pub fn layout_for_policy(policy: &PolicyDraft, network: &str) -> SlotLayout {
    let path = default_derivation_path(policy, network, 0);
    let slot = |id: String, label: String, path_id: &str| Slot {
        id,
        label,
        path_id: path_id.to_string(),
        derivation_path: path.clone(),
        expected: None,
        filled: None,
    };

    match policy {
        PolicyDraft::Singlesig => SlotLayout {
            paths: vec![PathSpec {
                id: "primary".into(),
                title: "Spending key".into(),
                subtitle: "Signs every transaction".into(),
                threshold: 1,
                timelock_blocks: None,
                slot_ids: vec!["k0".into()],
            }],
            slots: vec![slot("k0".into(), "Key".into(), "primary")],
        },
        PolicyDraft::Multisig { threshold, n } => {
            let slots: Vec<Slot> = (0..*n as usize)
                .map(|i| slot(format!("k{i}"), format!("Key {}", letter(i)), "primary"))
                .collect();
            SlotLayout {
                paths: vec![PathSpec {
                    id: "primary".into(),
                    title: format!("{threshold}-of-{n} signers"),
                    subtitle: format!("Any {threshold} of these {n} keys can spend"),
                    threshold: *threshold,
                    timelock_blocks: None,
                    slot_ids: slots.iter().map(|s| s.id.clone()).collect(),
                }],
                slots,
            }
        }
        PolicyDraft::Timelocked {
            primary,
            recoveries,
            ..
        } => {
            let mut paths = Vec::new();
            let mut slots = Vec::new();
            let mut next = 0usize;
            if let Some(p) = primary {
                let mut ids = Vec::new();
                for i in 0..p.n as usize {
                    let id = format!("k{next}");
                    next += 1;
                    ids.push(id.clone());
                    let label = if p.n == 1 {
                        "Primary key".to_string()
                    } else {
                        format!("Primary key {}", letter(i))
                    };
                    slots.push(slot(id, label, "primary"));
                }
                paths.push(PathSpec {
                    id: "primary".into(),
                    title: "Primary path".into(),
                    subtitle: if p.n == 1 {
                        "Spends immediately".into()
                    } else {
                        format!("{}-of-{}, spends immediately", p.threshold, p.n)
                    },
                    threshold: p.threshold,
                    timelock_blocks: None,
                    slot_ids: ids,
                });
            }
            for (r, rec) in recoveries.iter().enumerate() {
                let path_id = format!("recovery_{r}");
                let mut ids = Vec::new();
                let prefix = if recoveries.len() == 1 {
                    "Recovery key".to_string()
                } else {
                    format!("Recovery {} key", r + 1)
                };
                for i in 0..rec.n as usize {
                    let id = format!("k{next}");
                    next += 1;
                    ids.push(id.clone());
                    let label = if rec.n == 1 {
                        prefix.clone()
                    } else {
                        format!("{prefix} {}", letter(i))
                    };
                    slots.push(slot(id, label, &path_id));
                }
                paths.push(PathSpec {
                    id: path_id,
                    title: if recoveries.len() == 1 {
                        "Recovery path".into()
                    } else {
                        format!("Recovery path {}", r + 1)
                    },
                    subtitle: format!(
                        "{}after {} of inactivity",
                        if rec.n == 1 {
                            String::new()
                        } else {
                            format!("{}-of-{}, ", rec.threshold, rec.n)
                        },
                        describe_timelock(rec.timelock_blocks)
                    ),
                    threshold: rec.threshold,
                    timelock_blocks: Some(rec.timelock_blocks),
                    slot_ids: ids,
                });
            }
            SlotLayout { paths, slots }
        }
    }
}

/// Lay out paths and pre-identified slots from a parsed descriptor.
pub fn layout_for_analysis(analysis: &DescriptorAnalysis) -> SlotLayout {
    let mut path_of: HashMap<String, String> = HashMap::new();
    for p in &analysis.paths {
        for id in &p.slot_ids {
            path_of.entry(id.clone()).or_insert_with(|| p.id.clone());
        }
    }

    let mut paths: Vec<PathSpec> = analysis
        .paths
        .iter()
        .map(|p| PathSpec {
            id: p.id.clone(),
            title: p.label.clone(),
            subtitle: if let Some(tl) = p.timelock_blocks {
                format!("After {} of inactivity", describe_timelock(tl))
            } else if p.threshold > 0 {
                if p.slot_ids.len() > 1 {
                    format!("Any {} of {} keys", p.threshold, p.slot_ids.len())
                } else {
                    "Spends immediately".into()
                }
            } else {
                "Custom script".into()
            },
            threshold: p.threshold,
            timelock_blocks: p.timelock_blocks,
            slot_ids: p.slot_ids.clone(),
        })
        .collect();

    // Keys that no path claims (shouldn't happen, but a custom script
    // may hide them) still get shown under a catch-all group.
    let orphaned: Vec<String> = analysis
        .slots
        .iter()
        .filter(|s| !path_of.contains_key(&s.id))
        .map(|s| s.id.clone())
        .collect();
    if !orphaned.is_empty() {
        paths.push(PathSpec {
            id: "other".into(),
            title: "Other keys".into(),
            subtitle: "Used elsewhere in the script".into(),
            threshold: 0,
            timelock_blocks: None,
            slot_ids: orphaned.clone(),
        });
        for id in orphaned {
            path_of.insert(id, "other".into());
        }
    }

    let slots = analysis
        .slots
        .iter()
        .enumerate()
        .map(|(i, s)| Slot {
            id: s.id.clone(),
            label: s.label.clone().unwrap_or_else(|| format!("Key {}", letter(i))),
            path_id: path_of.get(&s.id).cloned().unwrap_or_else(|| "other".into()),
            derivation_path: s.derivation_path.clone().unwrap_or_default(),
            expected: Some(s.clone()),
            filled: None,
        })
        .collect();
    SlotLayout { paths, slots }
}

#[cfg(test)]
fn is_hot(s: &Slot) -> bool {
    s.filled.as_ref().is_some_and(|f| f.secret.is_some())
}

/// Paths that keys stored on this computer could satisfy on their own.
/// Single-key wallets are excluded: holding the key is the point.
#[cfg(test)]
pub fn paths_spendable_from_this_device(layout: &SlotLayout) -> Vec<PathSpec> {
    paths_spendable_by(layout, is_hot)
}

/// Like [`paths_spendable_from_this_device`], with the caller deciding
/// which slots hold a private key here (library keys are only fetched
/// at submit time, so `filled.secret` is empty until then).
pub fn paths_spendable_by(layout: &SlotLayout, is_hot: impl Fn(&Slot) -> bool) -> Vec<PathSpec> {
    if layout.slots.len() <= 1 {
        return vec![];
    }
    layout
        .paths
        .iter()
        .filter(|p| {
            if p.threshold == 0 {
                return false;
            }
            let hot = p
                .slot_ids
                .iter()
                .filter(|id| layout.slot(id).is_some_and(&is_hot))
                .count();
            hot as u32 >= p.threshold
        })
        .cloned()
        .collect()
}

/// Another slot that already holds this exact key, if any.
#[allow(dead_code)] // setup/slot_editor.rs
pub fn duplicate_of<'a>(slots: &'a [Slot], slot_id: &str, xpub: &str) -> Option<&'a Slot> {
    slots.iter().find(|s| {
        s.id != slot_id && s.filled.as_ref().is_some_and(|f| f.key.xpub == xpub)
    })
}

pub fn to_wire_policy(policy: &PolicyDraft, layout: &SlotLayout) -> WirePolicy {
    let index: HashMap<&str, usize> = layout
        .slots
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.as_str(), i))
        .collect();
    let indices = |path_id: &str| -> Vec<usize> {
        layout
            .paths
            .iter()
            .find(|p| p.id == path_id)
            .map(|p| p.slot_ids.iter().filter_map(|id| index.get(id.as_str()).copied()).collect())
            .unwrap_or_default()
    };
    match policy {
        PolicyDraft::Singlesig => WirePolicy::Singlesig,
        PolicyDraft::Multisig { threshold, .. } => WirePolicy::Multisig {
            threshold: *threshold,
        },
        PolicyDraft::Timelocked {
            script,
            primary,
            recoveries,
        } => WirePolicy::Timelocked {
            script: *script,
            primary: primary.map(|p| WirePath {
                threshold: p.threshold,
                slots: indices("primary"),
            }),
            recoveries: recoveries
                .iter()
                .enumerate()
                .map(|(r, rec)| WireRecovery {
                    timelock_blocks: rec.timelock_blocks,
                    threshold: rec.threshold,
                    slots: indices(&format!("recovery_{r}")),
                })
                .collect(),
        },
    }
}

/// Validate a draft before laying it out. Returns a human message or None.
pub fn policy_problem(policy: &PolicyDraft) -> Option<String> {
    match policy {
        PolicyDraft::Singlesig => None,
        PolicyDraft::Multisig { threshold, n } => {
            if *n < 2 || *n > 15 {
                return Some("Multisig needs between 2 and 15 keys.".into());
            }
            if *threshold < 1 || threshold > n {
                return Some("Threshold must be between 1 and the number of keys.".into());
            }
            None
        }
        PolicyDraft::Timelocked {
            script,
            primary,
            recoveries,
        } => {
            if primary.is_none() && *script == ScriptKind::Segwit {
                return Some("A recovery-only wallet needs Taproot: SegWit v0 has no unspendable key to put in place of the primary path.".into());
            }
            if let Some(p) = primary {
                if p.threshold < 1 || p.threshold > p.n {
                    return Some(
                        "Primary threshold must be between 1 and its number of keys.".into(),
                    );
                }
            }
            if recoveries.is_empty() {
                return Some("Add at least one recovery path.".into());
            }
            let mut seen = HashSet::new();
            for (i, rec) in recoveries.iter().enumerate() {
                if rec.timelock_blocks < 1 || rec.timelock_blocks > MAX_TIMELOCK_BLOCKS {
                    return Some(format!(
                        "Recovery path {}: timelock must be between 1 and {MAX_TIMELOCK_BLOCKS} blocks.",
                        i + 1
                    ));
                }
                if rec.threshold < 1 || rec.threshold > rec.n {
                    return Some(format!(
                        "Recovery path {}: threshold must be between 1 and its number of keys.",
                        i + 1
                    ));
                }
                if !seen.insert(rec.timelock_blocks) {
                    return Some("Two recovery paths can't share the same timelock. Give each a different delay.".into());
                }
            }
            None
        }
    }
}

pub fn policy_summary(policy: &PolicyDraft) -> String {
    match policy {
        PolicyDraft::Singlesig => "Single key".into(),
        PolicyDraft::Multisig { threshold, n } => format!("{threshold}-of-{n} multisig"),
        PolicyDraft::Timelocked {
            script,
            primary,
            recoveries,
        } => {
            let recs = recoveries.len();
            let base = if primary.is_some() {
                "Timelocked recovery"
            } else {
                "Recovery-only"
            };
            format!(
                "{base} · {recs} recovery path{} · {}",
                if recs == 1 { "" } else { "s" },
                if *script == ScriptKind::Taproot {
                    "Taproot"
                } else {
                    "SegWit v0"
                }
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TPUB: &str = "tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp";

    fn hot(fp: &str) -> FilledKey {
        FilledKey {
            source: SlotSource::Seed,
            key: SlotKey {
                fingerprint: fp.into(),
                derivation_path: "m/48'/1'/0'/2'".into(),
                xpub: TPUB.into(),
            },
            secret: Some(HotSecret::Mnemonic {
                mnemonic: "x".into(),
                bip39_passphrase: String::new(),
            }),
            detail: None,
        }
    }

    fn cold(fp: &str) -> FilledKey {
        FilledKey {
            source: SlotSource::Hardware,
            secret: None,
            ..hot(fp)
        }
    }

    #[test]
    fn layout_uses_bip84_for_single_key_and_bip48_for_multi_key() {
        assert_eq!(
            layout_for_policy(&PolicyDraft::Singlesig, "signet").slots[0].derivation_path,
            "m/84'/1'/0'"
        );
        let ms = layout_for_policy(&PolicyDraft::Multisig { threshold: 2, n: 3 }, "regtest");
        assert!(ms.slots.iter().all(|s| s.derivation_path == "m/48'/1'/0'/2'"));
        assert_eq!(ms.slots.len(), 3);
        assert_eq!(ms.paths[0].slot_ids, vec!["k0", "k1", "k2"]);
    }

    #[test]
    fn layout_numbers_timelocked_slots_across_paths_and_maps_wire_indices() {
        let policy = PolicyDraft::Timelocked {
            script: ScriptKind::Taproot,
            primary: Some(Threshold { threshold: 2, n: 2 }),
            recoveries: vec![
                RecoveryDraft { timelock_blocks: 4320, threshold: 1, n: 1 },
                RecoveryDraft { timelock_blocks: 52560, threshold: 2, n: 3 },
            ],
        };
        let layout = layout_for_policy(&policy, "regtest");
        assert_eq!(layout.slots.len(), 6);
        assert_eq!(
            to_wire_policy(&policy, &layout),
            WirePolicy::Timelocked {
                script: ScriptKind::Taproot,
                primary: Some(WirePath { threshold: 2, slots: vec![0, 1] }),
                recoveries: vec![
                    WireRecovery { timelock_blocks: 4320, threshold: 1, slots: vec![2] },
                    WireRecovery { timelock_blocks: 52560, threshold: 2, slots: vec![3, 4, 5] },
                ],
            }
        );
    }

    #[test]
    fn layout_leaves_the_primary_out_of_a_recovery_only_vault() {
        let policy = PolicyDraft::Timelocked {
            script: ScriptKind::Taproot,
            primary: None,
            recoveries: vec![RecoveryDraft { timelock_blocks: 4320, threshold: 1, n: 1 }],
        };
        let layout = layout_for_policy(&policy, "regtest");
        assert_eq!(
            layout.paths.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            vec!["recovery_0"]
        );
        match to_wire_policy(&policy, &layout) {
            WirePolicy::Timelocked { primary, .. } => assert_eq!(primary, None),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn wire_policy_serializes_like_the_ts_shape() {
        let v = serde_json::to_value(WirePolicy::Timelocked {
            script: ScriptKind::Segwit,
            primary: None,
            recoveries: vec![],
        })
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({"kind": "timelocked", "script": "segwit", "primary": null, "recoveries": []})
        );
    }

    #[test]
    fn policy_problem_rejects_recovery_paths_sharing_a_timelock() {
        let p = policy_problem(&PolicyDraft::Timelocked {
            script: ScriptKind::Taproot,
            primary: Some(Threshold { threshold: 1, n: 1 }),
            recoveries: vec![
                RecoveryDraft { timelock_blocks: 100, threshold: 1, n: 1 },
                RecoveryDraft { timelock_blocks: 100, threshold: 1, n: 1 },
            ],
        });
        assert!(p.unwrap().contains("same timelock"));
    }

    #[test]
    fn policy_problem_rejects_an_unspendable_primary_on_segwit_v0() {
        let p = policy_problem(&PolicyDraft::Timelocked {
            script: ScriptKind::Segwit,
            primary: None,
            recoveries: vec![RecoveryDraft { timelock_blocks: 100, threshold: 1, n: 1 }],
        });
        assert!(p.unwrap().contains("Taproot"));
    }

    #[test]
    fn policy_problem_accepts_the_common_presets() {
        assert_eq!(policy_problem(&PolicyDraft::Singlesig), None);
        assert_eq!(policy_problem(&PolicyDraft::Multisig { threshold: 2, n: 3 }), None);
        for p in common_presets() {
            assert_eq!(policy_problem(&p.draft), None);
        }
    }

    #[test]
    fn spendable_flags_a_2of3_with_two_local_keys_but_not_with_one() {
        let mut layout = layout_for_policy(&PolicyDraft::Multisig { threshold: 2, n: 3 }, "regtest");
        layout.slots[0].filled = Some(hot("aaaaaaaa"));
        layout.slots[1].filled = Some(cold("bbbbbbbb"));
        assert!(paths_spendable_from_this_device(&layout).is_empty());
        layout.slots[1].filled = Some(hot("bbbbbbbb"));
        assert_eq!(
            paths_spendable_from_this_device(&layout)
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            vec!["primary"]
        );
    }

    #[test]
    fn spendable_never_flags_a_single_key_wallet() {
        let mut layout = layout_for_policy(&PolicyDraft::Singlesig, "regtest");
        layout.slots[0].filled = Some(hot("aaaaaaaa"));
        assert!(paths_spendable_from_this_device(&layout).is_empty());
    }

    #[test]
    fn with_account_bumps_the_account_of_standard_paths_only() {
        assert_eq!(with_account("m/48'/1'/0'/2'", 3).as_deref(), Some("m/48'/1'/3'/2'"));
        assert_eq!(with_account("m/84'/0'/0'", 1).as_deref(), Some("m/84'/0'/1'"));
        assert_eq!(with_account("m/0/1", 1), None);
    }

    #[test]
    fn describe_timelock_buckets() {
        assert_eq!(describe_timelock(100), "100 blocks");
        assert_eq!(describe_timelock(144), "~1 day");
        assert_eq!(describe_timelock(1008), "~7 days");
        assert_eq!(describe_timelock(25920), "~6 months");
        assert_eq!(describe_timelock(52560), "~12 months");
    }
}
