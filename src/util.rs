use std::time::Duration;

use poise::serenity_prelude::*;
use serde::{Deserialize, Deserializer};
use tracing::error;

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

/// Gets the icon URL of a user.
///
/// If the user is not found, returns a default icon URL.
pub async fn get_icon_url(
    cache_http: impl CacheHttp,
    guild_id: GuildId,
    user_id: UserId,
) -> String {
    guild_id
        .member(cache_http, user_id)
        .await
        .map(|member| member.face())
        .unwrap_or_else(|e| {
            error!("Failed to get member: {e:#}");
            format!(
                "https://cdn.discordapp.com/embed/avatars/{}.png",
                ((user_id.get() >> 22) % 6) as u16
            )
        })
}
