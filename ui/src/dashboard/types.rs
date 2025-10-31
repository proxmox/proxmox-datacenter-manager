use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum LeaderboardType {
    GuestCpu,
    NodeCpu,
    NodeMemory,
}
