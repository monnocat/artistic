use std::{fs, time::Duration};

use chrono::{NaiveTime, Weekday};
use color_eyre::{
    Result,
    eyre::{Context, eyre},
};
use figment::{
    Figment,
    providers::{Format, Toml},
};
use poise::serenity_prelude::*;
use serde::Deserialize;

use crate::util::deserialize_duration;

/// The configuration for the bot.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// The operating guild ID.
    #[serde(rename = "guild-id")]
    pub guild: GuildId,

    /// The internal artist channel ID.
    #[serde(rename = "internal-channel-id")]
    pub internal_channel: ChannelId,

    /// The internal artist poll channel ID.
    #[serde(rename = "internal-poll-channel-id")]
    pub internal_poll_channel: ChannelId,

    /// The external artist channel ID.
    #[serde(rename = "external-channel-id")]
    pub external_channel: ChannelId,

    /// The external artist poll channel ID.
    #[serde(rename = "external-poll-channel-id")]
    pub external_poll_channel: ChannelId,

    /// The form timeout duration in seconds.
    ///
    /// If the form is not submitted within this duration, it will be cancelled.
    #[serde(rename = "form-timeout")]
    #[serde(deserialize_with = "deserialize_duration")]
    pub form_timeout: Duration,

    /// The weekday to post announcements on.
    #[serde(rename = "announcement-weekday")]
    pub announcement_weekday: Weekday,

    /// The time of day to post announcements at (UTC).
    #[serde(rename = "announcement-time")]
    pub announcement_time: NaiveTime,

    /// The role ID to ping in announcements.
    #[serde(rename = "announcement-role-id")]
    pub announcement_role: RoleId,

    /// The minimum number of votes required to pass a poll.
    #[serde(rename = "poll-threshold")]
    pub poll_threshold: usize,

    /// The poll facilitator role ID.
    #[serde(rename = "facilitator-role-id")]
    pub facilitator_role: RoleId,

    /// The duration in seconds between announcements.
    /// If not zero, announcements will be sent every `announcement-period` seconds instead of the normal schedule.
    ///
    /// This is used for testing.
    #[serde(rename = "debug-announcement-period")]
    #[serde(deserialize_with = "deserialize_duration")]
    #[serde(default)]
    pub debug_announcement_period: Duration,
}

impl Config {
    pub fn load() -> Result<Config> {
        if !fs::exists("./data/config.toml")
            .wrap_err("failed to check if ./data/config.toml exists")?
        {
            fs::write(
                "./data/config.toml",
                include_str!("../assets/default-config.toml"),
            )
            .wrap_err("failed to write ./data/config.toml")?;
            return Err(eyre!(
                "./data/config.toml not found, created default config"
            ));
        }

        Figment::new()
            .merge(Toml::file_exact("./data/config.toml"))
            .extract::<Config>()
            .wrap_err("failed to load config")
    }
}
