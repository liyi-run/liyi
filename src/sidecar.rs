use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level `.liyi.jsonc` file representation
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SidecarFile {
    pub version: String,
    pub source: String,
    pub specs: Vec<Spec>,
}

/// Represents either an item or a requirement spec
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Spec {
    Item(ItemSpec),
    Requirement(RequirementSpec),
}

/// Details of a code item (function, struct, etc.)
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemSpec {
    pub item: String,
    
    #[serde(default)]
    pub reviewed: bool,
    
    pub intent: String,
    
    pub source_span: [usize; 2],
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_anchor: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related: Option<HashMap<String, Option<String>>>,
}

/// Details of a module requirement/invariant
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementSpec {
    pub requirement: String,
    
    pub source_span: [usize; 2],
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_anchor: Option<String>,
}