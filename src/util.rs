use std::time::Duration;

use poise::serenity_prelude::*;
use serde::{Deserialize, Deserializer};

pub fn artist(internal: bool) -> &'static str {
    if internal { "internal" } else { "external" }
}

pub fn artist_capital(internal: bool) -> &'static str {
    if internal {
        "Biweekly Internal"
    } else {
        "Weekly External"
    }
}

pub fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Duration::from_secs(u64::deserialize(deserializer)?))
}
