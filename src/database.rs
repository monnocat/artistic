use color_eyre::{
    Result,
    eyre::{Context, eyre},
};
use futures::StreamExt;
use itertools::Itertools;
use poise::serenity_prelude::{MessageId, UserId};
use sqlx::{Executor, SqlitePool, query, sqlite::SqliteConnectOptions};

use crate::{
    types::{Poll, PollStatus, Suggestion},
    util::artist,
};

/// Connects to the database, creating it if it doesn't exist.
pub async fn connect() -> Result<SqlitePool> {
    let pool = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename("./data/database.sqlite")
            .create_if_missing(true),
    )
    .await
    .wrap_err("failed to connect to ./data/database.sqlite")?;

    create_tables(&pool).await?;

    Ok(pool)
}

/// Creates the tables if they don't exist.
pub async fn create_tables(pool: &SqlitePool) -> Result<()> {
    let mut stream = pool.execute_many(query(include_str!("../assets/create-tables.sql")));

    while let Some(result) = stream.next().await {
        result.wrap_err("failed to create table")?;
    }

    Ok(())
}

/// Adds a new suggestion to the database.
pub async fn insert_suggestion(
    pool: &SqlitePool,
    suggestion: &Suggestion,
    poll_id: u64,
) -> Result<()> {
    let user_id = suggestion.user_id.get() as i64;
    let poll_id = poll_id as i64;

    query!(
        "INSERT INTO suggestions (user_id, username, artist_name, album_name, links, notes, internal, poll_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        user_id,
        suggestion.username,
        suggestion.artist_name,
        suggestion.album_name,
        suggestion.links,
        suggestion.notes,
        suggestion.internal,
        poll_id
    )
    .execute(pool)
    .await
    .wrap_err("failed to insert suggestion")?;

    Ok(())
}

/// Fetches a suggestion by its poll ID.
pub async fn fetch_suggestion(pool: &SqlitePool, poll_id: u64) -> Result<Suggestion> {
    let poll_id = poll_id as i64;

    let suggestion = query!(
        "SELECT id, user_id, username, artist_name, album_name, links, notes, internal
         FROM suggestions
         WHERE poll_id = ?",
        poll_id
    )
    .fetch_optional(pool)
    .await
    .wrap_err("failed to fetch suggestion")?
    .ok_or(eyre!("suggestion not found"))?;

    Ok(Suggestion {
        id: suggestion.id as u64,
        user_id: UserId::new(suggestion.user_id as u64),
        username: suggestion.username,
        artist_name: suggestion.artist_name,
        album_name: suggestion.album_name,
        links: suggestion.links,
        notes: suggestion.notes,
        internal: suggestion.internal,
    })
}

/// Approves the suggestion with the given poll ID.
pub async fn approve_suggestion(pool: &SqlitePool, poll_id: u64) -> Result<()> {
    let poll_id = poll_id as i64;

    query!(
        "UPDATE suggestions
         SET approved = TRUE
         WHERE poll_id = ?",
        poll_id
    )
    .execute(pool)
    .await
    .wrap_err("failed to update suggestion")?;

    Ok(())
}

/// Fetches the oldest approved suggestion.
pub async fn pick_suggestion(pool: &SqlitePool, internal: bool) -> Result<Suggestion> {
    let suggestion = query!(
        "SELECT id, user_id, username, artist_name, album_name, links, notes, internal
         FROM suggestions
         WHERE internal = ? AND approved = TRUE
         GROUP BY user_id
         ORDER BY timestamp
         LIMIT 1",
        internal
    )
    .fetch_optional(pool)
    .await
    .wrap_err("failed to fetch suggestion")?
    .ok_or(eyre!("no approved {} suggestion found", artist(internal)))?;

    Ok(Suggestion {
        id: suggestion.id as u64,
        user_id: UserId::new(suggestion.user_id as u64),
        username: suggestion.username,
        artist_name: suggestion.artist_name,
        album_name: suggestion.album_name,
        links: suggestion.links,
        notes: suggestion.notes,
        internal: suggestion.internal,
    })
}

/// Removes the suggestion with the given suggestion ID.
pub async fn remove_suggestion(pool: &SqlitePool, suggestion_id: u64) -> Result<()> {
    let suggestion_id = suggestion_id as i64;

    query!(
        "DELETE FROM suggestions
         WHERE id = ?",
        suggestion_id
    )
    .execute(pool)
    .await
    .wrap_err("failed to remove suggestion")?;

    Ok(())
}

/// Removes the suggestion with the given poll ID.
pub async fn remove_suggestion_by_poll(pool: &SqlitePool, poll_id: u64) -> Result<()> {
    let poll_id = poll_id as i64;

    query!(
        "DELETE FROM suggestions
         WHERE poll_id = ?",
        poll_id
    )
    .execute(pool)
    .await
    .wrap_err("failed to remove suggestion")?;

    Ok(())
}

/// Inserts a new poll into the database and returns its ID.
pub async fn insert_poll(
    pool: &SqlitePool,
    message_id: MessageId,
    author_id: UserId,
    internal: bool,
) -> Result<u64> {
    let message_id = message_id.get() as i64;
    let author_id = author_id.get() as i64;

    Ok(
        query!(
            "INSERT INTO polls (message_id, author_id, internal)
             VALUES (?, ?, ?)",
            message_id,
            author_id,
            internal
        )
        .execute(pool)
        .await
        .wrap_err("failed to insert poll")?
        .last_insert_rowid() as u64, // this is the same as `id`, because it is an alias for `rowid`
    )
}

/// Fetches all polls from the database.
pub async fn fetch_polls(pool: &SqlitePool) -> Result<Vec<Poll>> {
    query!(
        "SELECT id, message_id, author_id, internal, status, votes
         FROM polls"
    )
    .fetch_all(pool)
    .await
    .wrap_err("failed to fetch polls")?
    .into_iter()
    .map(|row| {
        Ok(Poll {
            id: row.id as u64,
            message_id: MessageId::new(row.message_id as u64),
            author_id: UserId::new(row.author_id as u64),
            internal: row.internal,
            status: PollStatus::parse(row.status as u64, row.votes)?,
        })
    })
    .collect::<Result<_>>()
}

/// Updates the status of the poll with the given ID.
pub async fn update_poll_status(
    pool: &SqlitePool,
    poll_id: u64,
    status: &PollStatus,
) -> Result<()> {
    let poll_id = poll_id as i64;
    let (status, votes) = match status {
        PollStatus::Pending { votes } => (0, Some(votes.iter().map(|id| id.to_string()).join(","))),
        PollStatus::Completed => (1, None),
        PollStatus::Revoked => (2, None),
        PollStatus::Vetoed => (3, None),
    };

    query!(
        "UPDATE polls
         SET status = ?, votes = ?
         WHERE id = ?",
        status,
        votes,
        poll_id
    )
    .execute(pool)
    .await
    .wrap_err("failed to update poll votes")?;

    Ok(())
}

/// Removes the poll with the given poll ID.
pub async fn remove_poll(pool: &SqlitePool, poll_id: u64) -> Result<()> {
    let poll_id = poll_id as i64;

    query!(
        "DELETE FROM polls
         WHERE id = ?",
        poll_id
    )
    .execute(pool)
    .await
    .wrap_err("failed to remove poll")?;

    Ok(())
}
