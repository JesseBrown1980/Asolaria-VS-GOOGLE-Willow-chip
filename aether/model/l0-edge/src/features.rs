//! Node features: `extractNodeFeatures` (vendor/legacy/gnnDataExport.js:25-64) and the
//! model-input scaling of `build_node_features` (vendor/gnn-sidecar/train_baseline.py:26-39).

use crate::event::Event;
use crate::jsnum::Dbl;
use alloc::{borrow::ToOwned, string::String, vec::Vec};
#[cfg(not(feature = "std"))]
use alloc::collections::BTreeMap as HashMap;
#[cfg(feature = "std")]
use std::collections::HashMap;

/// One row of `extractNodeFeatures`; avgRisk and failureRate are held as their rounded integers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeFeatures {
    /// `id`: the event's id, or "unknown" when falsy.
    pub id: String,
    /// `type` at the node's first sight, "unknown" when falsy.
    pub node_type: String,
    /// `edgeVolume` (edgeCount).
    pub edge_volume: u64,
    /// `Math.round(totalRisk / edgeCount * 100)`; avgRisk is this / 100.
    pub avg_risk_centi: u64,
    /// `maxRisk`.
    pub max_risk: u32,
    /// `Math.round(failures / edgeCount * 1000)`; failureRate is this / 1000.
    pub failure_rate_milli: u64,
    /// `online`, always 1.
    pub online: u8,
    /// `trustTier`: "runtime" 3, "local_client" 2, else 1.
    pub trust_tier: u8,
}

/// Fractional bits of the model's fixed-point inputs (Q24).
pub const FRAC_BITS: u32 = 24;

const UNKNOWN: &str = "unknown";

struct NodeTally {
    id: String,
    node_type: String,
    edge_count: u64,
    total_risk: u64,
    max_risk: u32,
    failures: u64,
}

fn js_round_rate(count: u64, edge_count: u64, scale: u64) -> u64 {
    if edge_count == 0 {
        return 0;
    }
    (Dbl::from_ratio(count, edge_count) * Dbl::from_u64(scale)).js_round()
}

fn trust_tier(node_type: &str) -> u8 {
    match node_type {
        "runtime" => 3,
        "local_client" => 2,
        _ => 1,
    }
}

/// `extractNodeFeatures(events)`: one row per node id, in first-sight order (the JS Map's order).
pub fn extract_node_features(events: &[Event]) -> Vec<NodeFeatures> {
    let mut tallies: Vec<NodeTally> = Vec::new();
    let mut slot_by_id: HashMap<String, usize> = HashMap::new();
    for event in events {
        let actor_id = event.actor_id.as_deref().unwrap_or(UNKNOWN);
        let target_id = event.target_id.as_deref().unwrap_or(UNKNOWN);
        for id in [actor_id, target_id] {
            let slot = match slot_by_id.get(id) {
                Some(&slot) => slot,
                None => {
                    let node_type = if event.actor_id.as_deref() == Some(id) {
                        event.actor_type.as_deref()
                    } else {
                        event.target_type.as_deref()
                    };
                    tallies.push(NodeTally {
                        id: id.to_owned(),
                        node_type: node_type.unwrap_or(UNKNOWN).to_owned(),
                        edge_count: 0,
                        total_risk: 0,
                        max_risk: 0,
                        failures: 0,
                    });
                    slot_by_id.insert(id.to_owned(), tallies.len() - 1);
                    tallies.len() - 1
                }
            };
            let tally = &mut tallies[slot];
            tally.edge_count += 1;
            let risk = event.risk_score;
            tally.total_risk += u64::from(risk);
            if risk > tally.max_risk {
                tally.max_risk = risk;
            }
            if matches!(event.status.as_deref(), Some("failed" | "error")) {
                tally.failures += 1;
            }
        }
    }
    tallies
        .into_iter()
        .map(|tally| NodeFeatures {
            avg_risk_centi: js_round_rate(tally.total_risk, tally.edge_count, 100),
            failure_rate_milli: js_round_rate(tally.failures, tally.edge_count, 1000),
            trust_tier: trust_tier(&tally.node_type),
            id: tally.id,
            node_type: tally.node_type,
            edge_volume: tally.edge_count,
            max_risk: tally.max_risk,
            online: 1,
        })
        .collect()
}

fn q24_ratio(num: u64, den: u64) -> i64 {
    let twice_scaled = u128::from(num) << (FRAC_BITS + 1);
    let den = u128::from(den);
    i64::try_from((twice_scaled + den) / (2 * den)).unwrap_or(i64::MAX)
}

/// train_baseline.py scaling in Q24, round half up:
/// [min(vol/100,1), avgRisk/9, maxRisk/9, failureRate, online, trustTier/3]
pub fn model_input_q24(f: &NodeFeatures) -> [i64; 6] {
    [
        q24_ratio(f.edge_volume.min(100), 100),
        q24_ratio(f.avg_risk_centi, 900),
        q24_ratio(u64::from(f.max_risk), 9),
        q24_ratio(f.failure_rate_milli, 1000),
        q24_ratio(u64::from(f.online), 1),
        q24_ratio(u64::from(f.trust_tier), 3),
    ]
}
