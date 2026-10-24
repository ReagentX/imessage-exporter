use askama::Template;

use imessage_database::{
    message_types::{
        app::AppMessage, app_store::AppStoreMessage, business_chat::BusinessMessage,
        collaboration::CollaborationMessage, digital_touch::DigitalTouchMessage,
        handwriting::HandwrittenMessage, music::MusicMessage, placemark::PlacemarkMessage,
        polls::Poll, url::URLMessage,
    },
    tables::{attachment::Attachment, messages::Message},
};

use crate::{
    app::compatibility::attachment_manager::AttachmentManagerMode,
    exporters::{
        formatter::BalloonFormatter,
        shared::{balloon::resolve_check_in_footer, render::render_template},
        txt::{
            TXT,
            view_model::{
                AppStoreVM, ApplePayVM, CheckInVM, CollaborationVM, FindMyVM, FitnessVM,
                FormAnswerVM, FormRequestVM, FormResponseVM, GenericAppVM, ListPickerItemVM,
                ListPickerVM, MusicVM, PlacemarkVM, PollOptionVM, PollVM, QuickReplyOptionVM,
                QuickReplyVM, SlideshowVM, UrlVM,
            },
        },
    },
};

/// Render a balloon template. Multi-line templates emit a `\n` after each
/// conditionally-included field, which leaves a trailing newline once the
/// last present field has been written; this helper trims it so every
/// balloon is returned in the same "no trailing newline" shape. Single-line
/// templates produce no trailing newline and pass through unchanged.
fn render_balloon<T: Template>(template: &T) -> String {
    let mut out = render_template(template);
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

// MARK: Balloon
impl BalloonFormatter for TXT<'_> {
    fn format_url(&self, msg: &Message, balloon: &URLMessage) -> String {
        render_balloon(&UrlVM {
            primary: balloon.get_url().or(msg.text.as_deref()).into(),
            title: balloon.title.into(),
            summary: balloon.summary.into(),
        })
    }

    fn format_music(&self, balloon: &MusicMessage) -> String {
        render_balloon(&MusicVM {
            lyrics: balloon.lyrics.as_deref(),
            track_name: balloon.track_name.into(),
            album: balloon.album.into(),
            artist: balloon.artist.into(),
            url: balloon.url.into(),
        })
    }

    fn format_collaboration(&self, balloon: &CollaborationMessage) -> String {
        let name = balloon.app_name.or(balloon.bundle_id);
        let has_label = name.is_some_and(|n| !n.is_empty());
        render_balloon(&CollaborationVM {
            name: name.into(),
            has_label,
            title: balloon.title.into(),
            url: balloon.get_url().into(),
        })
    }

    fn format_app_store(&self, balloon: &AppStoreMessage) -> String {
        render_balloon(&AppStoreVM {
            app_name: balloon.app_name.into(),
            description: balloon.description.into(),
            platform: balloon.platform.into(),
            genre: balloon.genre.into(),
            url: balloon.get_url().into(),
        })
    }

    fn format_placemark(&self, balloon: &PlacemarkMessage) -> String {
        render_balloon(&PlacemarkVM {
            place_name: balloon.place_name.into(),
            url: balloon.get_url().into(),
            name: balloon.placemark.name.into(),
            address: balloon.placemark.address.into(),
            state: balloon.placemark.state.into(),
            city: balloon.placemark.city.into(),
            iso_country_code: balloon.placemark.iso_country_code.into(),
            postal_code: balloon.placemark.postal_code.into(),
            country: balloon.placemark.country.into(),
            street: balloon.placemark.street.into(),
            sub_administrative_area: balloon.placemark.sub_administrative_area.into(),
            sub_locality: balloon.placemark.sub_locality.into(),
        })
    }

    fn format_handwriting(&self, msg: &Message, balloon: &HandwrittenMessage) -> String {
        match self.config.options.attachment_manager.mode {
            AttachmentManagerMode::Disabled => balloon.render_ascii(40),
            _ => match self.config.options.attachment_manager.handle_handwriting(
                msg,
                balloon,
                self.config,
            ) {
                Ok(Some(path)) => self.config.relative_path(&path),
                Ok(None) => balloon.render_ascii(40),
                Err(e) => {
                    self.state.errors.set(self.state.errors.get() + 1);
                    eprintln!("Unable to save handwriting: {e}");
                    balloon.render_ascii(40)
                }
            },
        }
    }

    fn format_digital_touch(&self, _: &Message, balloon: &DigitalTouchMessage) -> String {
        balloon.render_text()
    }

    fn format_apple_pay(&self, balloon: &AppMessage) -> String {
        render_balloon(&ApplePayVM {
            caption: balloon.caption.into(),
            ldtext: balloon.ldtext.into(),
        })
    }

    fn format_fitness(&self, balloon: &AppMessage) -> String {
        render_balloon(&FitnessVM {
            app_name: balloon.app_name.into(),
            ldtext: balloon.ldtext.into(),
        })
    }

    fn format_slideshow(&self, balloon: &AppMessage) -> String {
        render_balloon(&SlideshowVM {
            ldtext: balloon.ldtext.into(),
            url: balloon.url.into(),
        })
    }

    fn format_find_my(&self, balloon: &AppMessage) -> String {
        render_balloon(&FindMyVM {
            app_name: balloon.app_name.into(),
            ldtext: balloon.ldtext.into(),
        })
    }

    fn format_check_in(&self, balloon: &AppMessage) -> String {
        render_balloon(&CheckInVM {
            caption: balloon.caption.unwrap_or("Check In"),
            footer: resolve_check_in_footer(balloon, &self.config.options.timezone),
        })
    }

    fn format_poll(&self, poll: &Poll) -> String {
        let options = poll
            .order
            .iter()
            .filter_map(|id| poll.options.get(id))
            .map(|opt| PollOptionVM {
                text: &opt.text,
                vote_count: opt.votes.len(),
                voters: opt.votes.iter().map(|v| v.voter.as_str()).collect(),
            })
            .collect();

        render_balloon(&PollVM { options })
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
                render_balloon(&QuickReplyVM {
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
                render_balloon(&FormResponseVM {
                    summary: form.summary.as_deref().into(),
                    answers,
                })
            }
            BusinessMessage::FormRequest(form) => render_balloon(&FormRequestVM {
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
                render_balloon(&ListPickerVM {
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
        _: &[Attachment],
        _: &Message,
    ) -> String {
        render_balloon(&GenericAppVM {
            name: balloon.app_name.unwrap_or(bundle_id),
            title: balloon.title.into(),
            subtitle: balloon.subtitle.into(),
            caption: balloon.caption.into(),
            subcaption: balloon.subcaption.into(),
            trailing_caption: balloon.trailing_caption.into(),
            trailing_subcaption: balloon.trailing_subcaption.into(),
        })
    }
}
