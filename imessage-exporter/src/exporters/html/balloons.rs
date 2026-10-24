use imessage_database::{
    message_types::{
        app::AppMessage,
        app_store::AppStoreMessage,
        business_chat::BusinessMessage,
        collaboration::CollaborationMessage,
        digital_touch::{DigitalTouchMessage, ImageBackdrop, media::MediaKind},
        handwriting::HandwrittenMessage,
        music::MusicMessage,
        placemark::PlacemarkMessage,
        polls::Poll,
        url::URLMessage,
    },
    tables::{
        attachment::{Attachment, MediaType},
        messages::{Message, models::AttachmentMeta},
    },
};

use crate::exporters::{
    formatter::{AttachmentRender, BalloonFormatter, MessageFormatter},
    html::{
        HTML,
        safe::Html,
        view_model::{
            AppCardVM, AppStoreVM, ApplePayVM, AttachmentVM, AttachmentVariant, CheckInVM,
            CollaborationVM, FindMyVM, FormAnswerVM, FormRequestVM, FormResponseVM,
            ListPickerItemVM, ListPickerVM, MusicVM, PlacemarkVM, PollOptionVM, PollVM,
            QuickReplyOptionVM, QuickReplyVM, UrlVM,
        },
    },
    shared::{
        attachment::{ExportFile, prepare_attachment},
        balloon::resolve_check_in_footer,
        driver::ExportState,
        render::render_template,
    },
};

use crate::app::runtime::Config;

/// Prepare the attachment backing a Digital Touch media message and confirm its
/// presence on disk. On `None`, render the labeled black canvas.
fn digital_touch_file<'a>(
    config: &Config,
    state: &ExportState,
    attachment: &'a Attachment,
    msg: &Message,
) -> Option<ExportFile<'a>> {
    let file = prepare_attachment(config, state, attachment, msg).ok()?;
    file.path.exists().then_some(file)
}

// MARK: Balloons
impl BalloonFormatter for HTML<'_> {
    fn format_url(&self, msg: &Message, balloon: &URLMessage) -> String {
        let balloon_url = balloon.get_url();
        let msg_text = msg.text.as_deref();
        render_template(&UrlVM {
            wrapper_url: balloon_url.or(msg_text).into(),
            name: balloon.site_name.or(balloon_url).or(msg_text).into(),
            title: balloon.title.into(),
            summary: balloon.summary.into(),
        })
    }

    fn format_music(&self, balloon: &MusicMessage) -> String {
        render_template(&MusicVM {
            track_name: balloon.track_name.into(),
            lyrics: balloon.lyrics.as_deref(),
            url: balloon.url.into(),
            artist: balloon.artist.into(),
            album: balloon.album.into(),
        })
    }

    fn format_collaboration(&self, balloon: &CollaborationMessage) -> String {
        render_template(&CollaborationVM {
            name: balloon.app_name.or(balloon.bundle_id).into(),
            wrapper_url: balloon.url.into(),
            title: balloon.title.into(),
            footer_url: balloon.get_url().into(),
        })
    }

    fn format_app_store(&self, balloon: &AppStoreMessage) -> String {
        render_template(&AppStoreVM {
            app_name: balloon.app_name.into(),
            url: balloon.get_url().into(),
            description: balloon.description.into(),
            platform: balloon.platform.into(),
            genre: balloon.genre.into(),
        })
    }

    fn format_placemark(&self, balloon: &PlacemarkMessage) -> String {
        render_template(&PlacemarkVM {
            place_name: balloon.place_name.into(),
            url: balloon.get_url().into(),
            name: balloon.placemark.name.into(),
            address: balloon.placemark.address.into(),
            postal_code: balloon.placemark.postal_code.into(),
            country: balloon.placemark.country.into(),
            sub_administrative_area: balloon.placemark.sub_administrative_area.into(),
        })
    }

    fn format_handwriting(&self, _: &Message, balloon: &HandwrittenMessage) -> String {
        balloon.render_svg()
    }

    fn format_digital_touch(&self, msg: &Message, balloon: &DigitalTouchMessage) -> String {
        // Wrap in `.digital_touch` (inside the standard `app` card) as the CSS
        // hook that caps the bubble to the card's width; the opaque content then
        // fills that bubble, covering the `app` card's white background.
        format!(
            r#"<div class="digital_touch">{}</div>"#,
            self.digital_touch_body(msg, balloon)
        )
    }

    fn format_apple_pay(&self, balloon: &AppMessage) -> String {
        render_template(&ApplePayVM {
            app_name: balloon.app_name.into(),
            ldtext: balloon.ldtext.into(),
        })
    }

    fn format_fitness(&self, balloon: &AppMessage) -> String {
        self.balloon_to_html(balloon, "Fitness", None)
    }

    fn format_slideshow(&self, balloon: &AppMessage) -> String {
        self.balloon_to_html(balloon, "Slideshow", None)
    }

    fn format_find_my(&self, balloon: &AppMessage) -> String {
        render_template(&FindMyVM {
            app_name: balloon.app_name.into(),
            ldtext: balloon.ldtext.into(),
        })
    }

    fn format_check_in(&self, balloon: &AppMessage) -> String {
        render_template(&CheckInVM {
            name: balloon.app_name.unwrap_or("Check In"),
            ldtext: balloon.ldtext.into(),
            footer: resolve_check_in_footer(balloon, &self.config.options.timezone),
        })
    }

    fn format_poll(&self, poll: &Poll) -> String {
        let max_votes = poll
            .order
            .iter()
            .filter_map(|option_id| poll.options.get(option_id))
            .map(|option| option.votes.len())
            .max()
            .unwrap_or(0);

        let options = poll
            .order
            .iter()
            .filter_map(|id| poll.options.get(id))
            .map(|opt| {
                let vote_count = opt.votes.len();
                let bar_width = (vote_count * 100).checked_div(max_votes).unwrap_or(0);
                PollOptionVM {
                    text: &opt.text,
                    vote_count,
                    bar_html: Html::trust(format!(
                        "<div class=\"vote-bar\" style=\"width: {bar_width}%;\"></div>"
                    )),
                    voters: opt.votes.iter().map(|v| v.voter.as_str()).collect(),
                }
            })
            .collect();

        render_template(&PollVM { options })
    }

    fn format_business(&self, balloon: &BusinessMessage) -> String {
        match balloon {
            BusinessMessage::QuickReply(quick_reply) => {
                let options = quick_reply
                    .options
                    .iter()
                    .enumerate()
                    .map(|(index, option)| QuickReplyOptionVM {
                        text: &option.title,
                        selected: quick_reply.selected_index == Some(index),
                    })
                    .collect();
                render_template(&QuickReplyVM {
                    summary: quick_reply.summary.as_deref().into(),
                    options,
                })
            }
            BusinessMessage::FormResponse(form) => {
                let answers = form
                    .answers
                    .iter()
                    .map(|answer| FormAnswerVM {
                        question: &answer.question,
                        answer: answer.answers.join(", "),
                    })
                    .collect();
                render_template(&FormResponseVM {
                    summary: form.summary.as_deref().into(),
                    answers,
                })
            }
            BusinessMessage::FormRequest(form) => render_template(&FormRequestVM {
                title: form.title.as_deref().into(),
                subtitle: form.subtitle.as_deref().into(),
            }),
            BusinessMessage::ListPicker(list_picker) => {
                let items = list_picker
                    .items
                    .iter()
                    .map(|item| ListPickerItemVM {
                        title: &item.title,
                        subtitle: item.subtitle.as_deref().into(),
                        selected: item.selected,
                    })
                    .collect();
                render_template(&ListPickerVM {
                    summary: list_picker.summary.as_deref().into(),
                    items,
                })
            }
        }
    }

    fn format_generic_app(
        &self,
        balloon: &AppMessage,
        bundle_id: &str,
        attachments: &[Attachment],
        msg: &Message,
    ) -> String {
        let attachment_html = if balloon.image.is_none() {
            attachments.first().map(|attachment| {
                let rendered =
                    match self.format_attachment(attachment, msg, &AttachmentMeta::default()) {
                        AttachmentRender::Embedded(html) => html,
                        AttachmentRender::MissingFilename | AttachmentRender::NamedFile(_) => {
                            String::new()
                        }
                    };
                Html::trust(rendered)
            })
        } else {
            None
        };
        self.balloon_to_html(balloon, bundle_id, attachment_html)
    }
}

impl HTML<'_> {
    /// Render the inner markup for a Digital Touch message, dispatching on the
    /// parsed [`MediaKind`]: a video plays as a standalone `<video>` (it never has
    /// an overlay, and an in-SVG `<foreignObject>` video overflows the bubble on
    /// play), while an image backs the SVG its overlay draws over. Anything with
    /// no usable backing attachment falls back to the labeled black canvas.
    fn digital_touch_body(&self, msg: &Message, balloon: &DigitalTouchMessage) -> String {
        let DigitalTouchMessage::Media(media) = balloon else {
            return balloon.render_svg(None);
        };
        // The media is a normal attachment on the message row.
        let Some(attachment) = Attachment::from_message(
            self.config.data_source.db(),
            msg,
            &self.config.data_source.capabilities,
        )
        .ok()
        .and_then(|rows| rows.into_iter().next()) else {
            return balloon.render_svg(None);
        };
        let Some(file) = digital_touch_file(self.config, &self.state, &attachment, msg) else {
            return balloon.render_svg(None);
        };
        let embed_path = self.config.relative_path(&file.path);

        // Dispatch on the parsed kind, but require the resolved file to match
        match (&media.kind, &file.media_type) {
            // Reuse the shared attachment `<video>` template so the player gets the
            // correct `type` hint and the duplicated source tag (see issue #73).
            (MediaKind::Video, MediaType::Video(media_type)) => render_template(&AttachmentVM {
                lazy: !self.config.options.no_lazy,
                embed_path,
                variant: AttachmentVariant::Video { media_type },
            }),
            (MediaKind::Image { .. }, MediaType::Image(_)) => {
                balloon.render_svg(Some(ImageBackdrop(embed_path.into())))
            }
            _ => balloon.render_svg(None),
        }
    }

    fn balloon_to_html(
        &self,
        balloon: &AppMessage,
        bundle_id: &str,
        attachment_html: Option<Html>,
    ) -> String {
        render_template(&AppCardVM {
            url: balloon.url.into(),
            attachment_html,
            name: balloon.app_name.unwrap_or(bundle_id),
            title: balloon.title.into(),
            subtitle: balloon.subtitle.into(),
            ldtext: balloon.ldtext.into(),
            caption: balloon.caption.into(),
            subcaption: balloon.subcaption.into(),
            trailing_caption: balloon.trailing_caption.into(),
            trailing_subcaption: balloon.trailing_subcaption.into(),
        })
    }
}
