use color_eyre::{Report, Result, eyre::Context as _};
use poise::{ApplicationContext, Command, command, serenity_prelude::*};

use crate::types::{Data, Suggestion};

/// Suggest an artist to be featured.
#[command(slash_command, guild_only, ephemeral)]
async fn suggest(
    ctx: ApplicationContext<'_, Data, Report>,

    #[description = "Artists in the C418 community are `internal`. All other artists are `external`."]
    #[choices("internal", "external")]
    artist: &'static str,
) -> Result<()> {
    let response = ctx.interaction
        .quick_modal(
            ctx.serenity_context,
            CreateQuickModal::new(format!("Suggest an {artist} artist!"))
                .field(
                    CreateInputText::new(InputTextStyle::Short, "Artist name", "")
                        .placeholder("The artist name")
                        .max_length(256),
                )
                .field(
                    CreateInputText::new(InputTextStyle::Short, "Album name", "")
                        .placeholder("The album name")
                        .max_length(256),
                )
                .field(
                    CreateInputText::new(InputTextStyle::Paragraph, "Links", "")
                        .placeholder("One or more links to the album on any platform.\nEach link should be on a new line.")
                        .max_length(1024)
                )
                .field(
                    CreateInputText::new(InputTextStyle::Paragraph, "Notes", "")
                        .placeholder("Any additional notes")
                        .max_length(1024)
                        .required(false),
                ).timeout(ctx.data.config.form_timeout),
        )
        .await?;

    let Some(response) = response else {
        return Ok(());
    };

    let respond_with_error = async {
        response
            .interaction
            .create_response(
                &ctx,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("There was an error processing your submission.")
                        .ephemeral(true),
                ),
            )
            .await
            .wrap_err("failed to send response")
    };

    // parse the response
    let suggestion = match Suggestion::parse_response(&response, artist == "internal")
        .wrap_err("failed to parse form response")
    {
        Ok(suggestion) => suggestion,
        Err(e) => {
            respond_with_error.await?;
            return Err(e);
        }
    };

    // create the poll
    let poll_id = match ctx
        .data
        .create_poll(ctx.serenity_context, &suggestion)
        .await
        .wrap_err("failed to create poll")
    {
        Ok(poll_id) => poll_id,
        Err(e) => {
            respond_with_error.await?;
            return Err(e);
        }
    };

    // add the suggestion to the database
    if let Err(e) = ctx
        .data
        .insert_suggestion(&suggestion, poll_id)
        .await
        .wrap_err("failed to insert suggestion")
    {
        respond_with_error.await?;
        return Err(e);
    }

    // respond to the submission
    response
        .interaction
        .create_response(
            &ctx,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content("Thanks for your suggestion!")
                    .ephemeral(true),
            ),
        )
        .await
        .wrap_err("failed to send response")?;

    Ok(())
}

pub fn get() -> Vec<Command<Data, Report>> {
    vec![suggest()]
}
