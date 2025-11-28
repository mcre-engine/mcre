use alloc::string::String;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum StateValue {
    Bool(bool),
    Int(u8),
    String(String),
}
