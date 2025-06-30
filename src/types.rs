use std::{collections::HashSet, sync::Arc};

use color_eyre::{
    Result,
    eyre::{Context as _, eyre},
};
use poise::serenity_prelude::*;
use sqlx::sqlite::SqlitePool;
use tokio::sync::Mutex;

use crate::{
    config::Config,
    database,
    util::{artist_capital, get_icon_url},
};

#[derive(Clone)]
pub struct Data {
    pub pool: SqlitePool,
    pub config: Config,
    pub polls: Arc<Mutex<Vec<Poll>>>, // this is here to avoid data races
                                      // suggestions are not used this way because they are not modified frequently
}

impl Data {
    pub async fn new(pool: SqlitePool, config: Config) -> Result<Data> {
        let polls = database::fetch_polls(&pool).await?;

        Ok(Data {
            pool,
            config,
            polls: Arc::new(Mutex::new(polls)),
        })
    }

    /// Returns the channel to post announcements to.
    pub fn get_channel(&self, internal: bool) -> ChannelId {
        if internal {
            self.config.internal_channel
        } else {
            self.config.external_channel
        }
    }

    /// Inserts a new suggestion into the database.
    pub async fn insert_suggestion(&self, suggestion: &Suggestion, poll_id: u64) -> Result<()> {
        database::insert_suggestion(&self.pool, suggestion, poll_id).await
    }

    /// Fetches a suggestion by its poll ID.
    pub async fn fetch_suggestion(&self, poll_id: u64) -> Result<Suggestion> {
        database::fetch_suggestion(&self.pool, poll_id)
            .await
            .wrap_err("failed to fetch suggestion")
    }

    /// Approves the suggestion with the given poll ID.
    pub async fn approve_suggestion(&self, poll_id: u64) -> Result<()> {
        database::approve_suggestion(&self.pool, poll_id)
            .await
            .wrap_err("failed to approve suggestion")
    }

    /// Removes the suggestion with the given poll ID.
    pub async fn remove_suggestion(&self, poll_id: u64) -> Result<()> {
        database::remove_suggestion(&self.pool, poll_id).await
    }

    /// Fetches the latest approved suggestion and removes it from the database.
    async fn pick_suggestion(&self, internal: bool) -> Result<Suggestion> {
        database::pick_suggestion(&self.pool, internal)
            .await
            .wrap_err("failed to pick suggestion")
    }

    /// Inserts a new poll to the database and returns its ID.
    async fn insert_poll(
        &self,
        message_id: MessageId,
        author_id: UserId,
        internal: bool,
    ) -> Result<u64> {
        let poll_id = database::insert_poll(&self.pool, message_id, author_id, internal).await?;
        let poll = Poll::new(poll_id, message_id, author_id, internal);
        self.polls.lock().await.push(poll);
        Ok(poll_id)
    }

    /// Updates the status of a poll by its ID.
    pub async fn update_poll_status(&self, poll_id: u64, status: &PollStatus) -> Result<()> {
        database::update_poll_status(&self.pool, poll_id, status).await
    }

    /// Removes the poll with the given poll ID.
    pub async fn remove_poll(&self, poll_id: u64) -> Result<()> {
        database::remove_poll(&self.pool, poll_id).await
    }

    pub async fn build_poll_embed(
        &self,
        cache_http: impl CacheHttp,
        suggestion: &Suggestion,
        status: &PollStatus,
    ) -> CreateEmbed {
        let (status, color) = status.format(self.config.poll_threshold);
        let icon_url = get_icon_url(&cache_http, self.config.guild, suggestion.user_id).await;

        let embed_author = CreateEmbedAuthor::new(suggestion.username.clone())
            .url(format!(
                "https://discordapp.com/users/{}",
                suggestion.user_id
            ))
            .icon_url(icon_url);

        let embed_title = format!(
            "{} Feature Artist Submission",
            artist_capital(suggestion.internal)
        );

        let mut embed_fields = vec![
            ("Artist Name", suggestion.artist_name.clone(), true),
            ("Album Name", suggestion.album_name.clone(), true),
            ("Album Link(s)", suggestion.links.clone(), false),
        ];

        if let Some(notes) = &suggestion.notes {
            embed_fields.push(("Other Comments", notes.clone(), false));
        }

        embed_fields.push(("Status", status, false));

        CreateEmbed::new()
            .author(embed_author)
            .title(embed_title)
            // .description("Users may upvote this submission with 👍")
            .fields(embed_fields)
            .color(color)
    }

    /// Creates a new poll for a suggestion and returns its ID.
    pub async fn create_poll(&self, ctx: &Context, suggestion: &Suggestion) -> Result<u64> {
        let embed = self
            .build_poll_embed(&ctx, suggestion, &PollStatus::default())
            .await;

        let components = vec![CreateActionRow::Buttons(vec![
            CreateButton::new("poll:upvote").label("Upvote").emoji('👍'),
            CreateButton::new("poll:revoke").label("Revoke").emoji('🗑'),
            CreateButton::new("poll:veto").label("Veto").emoji('🛑'),
        ])];

        let message_builder = CreateMessage::new()
            .content(format!(
                "<@{}> here's your new submission!",
                suggestion.user_id
            ))
            .add_embed(embed)
            .components(components);

        // send the poll
        let message = self
            .get_channel(suggestion.internal)
            .send_message(ctx, message_builder)
            .await
            .wrap_err("failed to send message")?;

        // add the poll
        let poll_id = self
            .insert_poll(message.id, suggestion.user_id, suggestion.internal)
            .await?;

        Ok(poll_id)
    }

    /// Fetches a suggestion from the database and posts it to the appropriate channel.
    pub async fn post_announcement(
        &self,
        cache_http: impl CacheHttp,
        internal: bool,
    ) -> Result<()> {
        let suggestion = self.pick_suggestion(internal).await?;

        let icon_url = get_icon_url(&cache_http, self.config.guild, suggestion.user_id).await;

        let embed_author = CreateEmbedAuthor::new(suggestion.username.clone())
            .url(format!(
                "https://discordapp.com/users/{}",
                suggestion.user_id
            ))
            .icon_url(icon_url);

        let embed_title = format!(
            "New {} Feature Artist! 🌟 🎵",
            artist_capital(suggestion.internal)
        );

        let mut embed_fields = vec![
            ("Artist Name", suggestion.artist_name.clone(), true),
            ("Album Name", suggestion.album_name.clone(), true),
            ("Album Link(s)", suggestion.links.clone(), false),
        ];

        if let Some(notes) = &suggestion.notes {
            embed_fields.push(("Other Comments", notes.clone(), false));
        }

        let embed = CreateEmbed::new()
            .author(embed_author)
            .title(embed_title)
            .fields(embed_fields)
            .color((87, 242, 135));

        self.get_channel(internal)
            .send_message(
                cache_http,
                CreateMessage::new()
                    .content(format!("<@&{}>", self.config.announcement_role))
                    .embed(embed),
            )
            .await
            .wrap_err("failed to send message")?;

        Ok(())
    }
}

pub struct Suggestion {
    pub user_id: UserId,
    pub username: String,
    pub artist_name: String,
    pub album_name: String,
    pub links: String,
    pub notes: Option<String>,
    pub internal: bool,
}

impl Suggestion {
    pub fn parse_response(response: &QuickModalResponse, internal: bool) -> Result<Suggestion> {
        if !(3..=4).contains(&response.inputs.len()) {
            return Err(eyre!("invalid form structure"));
        };

        Ok(Suggestion {
            user_id: response.interaction.user.id,
            username: response.interaction.user.name.clone(),
            artist_name: response.inputs[0].clone(),
            album_name: response.inputs[1].clone(),
            links: response.inputs[2].clone(),
            notes: response.inputs.get(3).cloned().filter(|s| !s.is_empty()),
            internal,
        })
    }
}

#[derive(Clone)]
pub struct Poll {
    pub id: u64,
    pub message_id: MessageId,
    pub author_id: UserId,
    pub internal: bool,
    pub status: PollStatus,
}

impl Poll {
    pub fn new(id: u64, message_id: MessageId, author_id: UserId, internal: bool) -> Poll {
        Poll {
            id,
            message_id,
            author_id,
            internal,
            status: PollStatus::default(),
        }
    }
}

#[derive(Clone)]
pub enum PollStatus {
    Pending { votes: HashSet<UserId> },
    Completed,
    Revoked,
    Vetoed,
}

impl PollStatus {
    pub fn parse(status: u64, votes: Option<String>) -> Result<PollStatus> {
        match (status, votes) {
            (0, Some(votes)) => Ok(PollStatus::Pending {
                votes: votes
                    .split_terminator(",")
                    .map(|id| id.parse().unwrap())
                    .collect(),
            }),
            (0, None) => Ok(PollStatus::Pending {
                votes: HashSet::new(),
            }),
            (1, None) => Ok(PollStatus::Completed),
            (2, None) => Ok(PollStatus::Revoked),
            (3, None) => Ok(PollStatus::Vetoed),
            _ => Err(eyre!("invalid poll status")),
        }
    }

    pub fn format(&self, threshold: usize) -> (String, Color) {
        match self {
            PollStatus::Pending { votes } => (
                format!("Pending ({}/{threshold}) 🗳️", votes.len()),
                Color::BLUE,
            ),
            PollStatus::Completed => ("Completed ✅".into(), Color::from_rgb(87, 242, 135)),
            PollStatus::Revoked => ("Revoked 🗑️".into(), Color::RED),
            PollStatus::Vetoed => ("Vetoed 🛑".into(), Color::RED),
        }
    }
}

impl Default for PollStatus {
    fn default() -> Self {
        PollStatus::Pending {
            votes: HashSet::new(),
        }
    }
}
