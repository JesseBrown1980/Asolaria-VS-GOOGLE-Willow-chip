//! Integer port of the Asolaria L0 EdgeLevelGNN, from observed graph events to an edge score.

#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod event;
pub mod features;
pub mod jsnum;
pub mod model;
pub mod weights;

use event::Event;
use features::{extract_node_features, model_input_q24};

/// Q24 score of the request edge `actor -> target`, node features from `events`;
/// `None` when either id was never observed.
pub fn observe_pair(events: &[Event], actor: &str, target: &str) -> Option<i64> {
    let nodes = extract_node_features(events);
    let input = |id: &str| nodes.iter().find(|node| node.id == id).map(model_input_q24);
    Some(model::score_pair(&input(actor)?, &input(target)?))
}
