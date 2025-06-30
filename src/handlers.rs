use chrono::{DateTime, Datelike, Days, NaiveTime, Utc, Weekday};
use color_eyre::{
    Result,
    eyre::{Context as _, Report, eyre},
};
use poise::{FrameworkContext, FrameworkError, serenity_prelude::*};
use tokio::{
    fs::{self, File},
    io::AsyncReadExt,
    time::sleep,
};
use tracing::error;

use crate::types::{Data, PollStatus};

/// Get the next instance of `weekday` at `time` UTC, including today, from `now`.
fn next_weekday_at(now: DateTime<Utc>, weekday: Weekday, time: NaiveTime) -> DateTime<Utc> {
    let today_at = now.with_time(time).unwrap();

    if today_at.weekday() == weekday && now < today_at {
        // If today is the given weekday and it hasn't passed yet, use today
        today_at
    } else {
        // If today is not the given weekday, or it has passed, find the next
        let mut day_offset = weekday.days_since(today_at.weekday());
        day_offset += (day_offset == 0) as u32 * 7; // if today is the given weekday, add a week
        today_at + Days::new(day_offset as u64)
    }
}

/// An infinite loop that posts internal and external artist announcements.
pub async fn post_announcements(ctx: Context, data: Data) {
    // get the biweekly flag from the file or create it
    let open_result = File::options()
        .read(true)
        .write(true)
        .truncate(false)
        .create(true)
        .open("biweekly_flag.bin")
        .await;

    let mut biweekly_flag = [0];

    match open_result {
        // the error is ignored because the file is created if it doesn't exist
        Ok(mut file) => _ = file.read_exact(&mut biweekly_flag).await,
        Err(e) => {
            error!("Failed to open biweekly_flag.bin: {e:#}");
        }
    };

    let mut biweekly_flag = biweekly_flag[0] % 2 == 0;

    loop {
        // reusing `now` because this could be called near the announcement time
        let now = Utc::now();
        let next_date = next_weekday_at(
            now,
            data.config.announcement_weekday,
            data.config.announcement_time,
        );

        // wait until the next announcement
        // unwrapping `to_std` is safe because `next_date` is always greater than `now`
        sleep((next_date - now).to_std().unwrap()).await;

        if let Err(e) = data.post_announcement(&ctx, false).await {
            error!("Failed to post external announcement: {e:#}");
        }

        if biweekly_flag {
            if let Err(e) = data.post_announcement(&ctx, true).await {
                error!("Failed to post internal announcement: {e:#}");
            }
        }

        biweekly_flag ^= true;

        if let Err(e) = fs::write("biweekly_flag.bin", [biweekly_flag as u8])
            .await
            .wrap_err("failed to write to biweekly_flag.bin")
        {
            error!("Failed to write to biweekly_flag.bin: {e:#}");
        }
    }
}

pub async fn event_handler(
    ctx: &Context,
    event: &FullEvent,
    _framework: FrameworkContext<'_, Data, Report>,
    data: &Data,
) -> Result<()> {
    if let FullEvent::InteractionCreate {
        interaction: Interaction::Component(interaction),
    } = event
    {
        if let Err(e) = handle_poll_interaction(ctx, interaction, data).await {
            error!("Failed to handle poll interaction: {e:#}");
            interaction
                .create_response(
                    &ctx,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content("There was an error processing your interaction.")
                            .ephemeral(true),
                    ),
                )
                .await?;
        }
    }

    Ok(())
}

async fn handle_poll_interaction(
    ctx: &Context,
    interaction: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    // check if the interaction is valid
    if !matches!(interaction.data.kind, ComponentInteractionDataKind::Button)
        || interaction.guild_id != Some(data.config.guild)
        || (interaction.channel_id != data.config.internal_channel
            && interaction.channel_id != data.config.external_channel)
        || !interaction.data.custom_id.starts_with("poll:")
    {
        return Ok(());
    }

    let mut polls = data.polls.lock().await;

    // get the poll if it exists
    let (poll_index, poll) = polls
        .iter_mut()
        .enumerate()
        .find(|(_, poll)| poll.message_id == interaction.message.id)
        .ok_or(eyre!("couldn't find poll"))?;

    let builder = CreateInteractionResponseMessage::new().ephemeral(true);

    // `strip_prefix` is safe because we checked that `interaction.data.custom_id` starts with "poll:"
    let response = match interaction.data.custom_id.strip_prefix("poll:").unwrap() {
        "upvote" => {
            if interaction.user.id != poll.author_id {
                match &mut poll.status {
                    PollStatus::Pending { votes } => {
                        let inserted = votes.insert(interaction.user.id);

                        // check if a new vote was added
                        if inserted {
                            let suggestion = data.fetch_suggestion(poll.id).await?;
                            let mut components = None;

                            // if the poll has enough votes, complete it
                            if votes.len() >= data.config.poll_threshold {
                                poll.status = PollStatus::Completed;

                                // approve the suggestion
                                data.approve_suggestion(poll.id).await?;

                                components = Some(vec![CreateActionRow::Buttons(vec![
                                    CreateButton::new("poll:upvote")
                                        .label("Upvote")
                                        .emoji('👍')
                                        .disabled(true),
                                    CreateButton::new("poll:revoke").label("Revoke").emoji('🗑'),
                                    CreateButton::new("poll:veto").label("Veto").emoji('🛑'),
                                ])]);
                            }

                            data.update_poll_status(poll.id, &poll.status).await?;

                            let embed =
                                data.build_poll_embed(&ctx, &suggestion, &poll.status).await;
                            let mut edit_builder = EditMessage::new().embed(embed);

                            if let Some(components) = components {
                                edit_builder = edit_builder.components(components);
                            }

                            // edit the message
                            data.get_channel(poll.internal)
                                .edit_message(&ctx, poll.message_id, edit_builder)
                                .await
                                .wrap_err("failed to edit message")?;

                            builder.content("Vote added!")
                        } else {
                            builder.content("You already voted!")
                        }
                    }
                    PollStatus::Completed => {
                        builder.content("This poll has already been completed!")
                    }
                    PollStatus::Revoked => builder.content("This poll has been revoked!"),
                    PollStatus::Vetoed => builder.content("This poll has been vetoed!"),
                }
            } else {
                builder.content("You can't vote on your own poll!")
            }
        }

        "revoke" => {
            if interaction.user.id == poll.author_id {
                match poll.status {
                    PollStatus::Pending { .. } | PollStatus::Completed => {
                        // revoke the poll
                        poll.status = PollStatus::Revoked;
                        let poll = polls.remove(poll_index);
                        data.remove_poll(poll.id).await?;

                        // remove the suggestion
                        let suggestion = data.fetch_suggestion(poll.id).await?;
                        data.remove_suggestion(poll.id).await?;

                        let embed = data.build_poll_embed(&ctx, &suggestion, &poll.status).await;

                        // edit the message
                        data.get_channel(poll.internal)
                            .edit_message(
                                &ctx,
                                poll.message_id,
                                EditMessage::new().embed(embed).components(Vec::new()),
                            )
                            .await
                            .wrap_err("failed to edit message")?;

                        builder.content("Poll revoked!")
                    }
                    PollStatus::Revoked => builder.content("This poll has already been revoked!"),
                    PollStatus::Vetoed => builder.content("This poll has been vetoed!"),
                }
            } else {
                builder.content("Only the author of the poll can revoke it!")
            }
        }

        "veto" => {
            if interaction
                .user
                .has_role(&ctx, data.config.guild, data.config.facilitator_role)
                .await
                .wrap_err("failed to check facilitator role")?
            {
                match poll.status {
                    PollStatus::Pending { .. } | PollStatus::Completed => {
                        // veto the poll
                        poll.status = PollStatus::Vetoed;
                        let poll = polls.remove(poll_index);
                        data.remove_poll(poll.id).await?;

                        // remove the suggestion
                        let suggestion = data.fetch_suggestion(poll.id).await?;
                        data.remove_suggestion(poll.id).await?;

                        let embed = data.build_poll_embed(&ctx, &suggestion, &poll.status).await;

                        // edit the message
                        data.get_channel(poll.internal)
                            .edit_message(
                                &ctx,
                                poll.message_id,
                                EditMessage::new().embed(embed).components(Vec::new()),
                            )
                            .await
                            .wrap_err("failed to edit message")?;

                        builder.content("Poll vetoed!")
                    }
                    PollStatus::Revoked => builder.content("This poll has been revoked!"),
                    PollStatus::Vetoed => builder.content("This poll has already been vetoed!"),
                }
            } else {
                builder.content("Only designated facilitators can veto polls!")
            }
        }

        _ => {
            return Err(eyre!(
                "unknown poll interaction: {}",
                interaction.data.custom_id
            ));
        }
    };

    interaction
        .create_response(&ctx, CreateInteractionResponse::Message(response))
        .await
        .wrap_err("failed to send response")?;

    Ok(())
}

pub async fn error_handler(err: FrameworkError<'_, Data, Report>) {
    match err {
        FrameworkError::Setup { error, .. } => error!("Setup error: {error:#}"),
        FrameworkError::EventHandler { error, .. } => error!("Event handler error: {error:#}"),
        FrameworkError::Command { error, .. } => error!("Command error: {error:#}"),
        FrameworkError::CommandPanic {
            payload: Some(payload),
            ..
        } => error!("Command panic: {payload}"),
        FrameworkError::ArgumentParse { error, .. } => error!("Argument parse error: {error:#}"),
        FrameworkError::CommandStructureMismatch { description, .. } => {
            error!("Command structure mismatch: {description}")
        }
        FrameworkError::CommandCheckFailed {
            error: Some(error), ..
        } => error!("Command check failed: {error:#}"),
        FrameworkError::DynamicPrefix { error, .. } => error!("Dynamic prefix error: {error:#}"),
        _ => {}
    }
}
