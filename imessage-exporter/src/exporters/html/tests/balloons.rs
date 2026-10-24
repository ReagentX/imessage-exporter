use std::{collections::HashMap, env::current_dir, fs::File, io::Read};

use crate::{
    Config, HTML, Options,
    app::export_type::ExportType,
    exporters::{formatter::BalloonFormatter, shared::test_support::config},
};

use imessage_database::message_types::{
    app::AppMessage,
    app_store::AppStoreMessage,
    collaboration::CollaborationMessage,
    digital_touch::DigitalTouchMessage,
    handwriting::HandwrittenMessage,
    music::MusicMessage,
    placemark::{Placemark, PlacemarkMessage},
    polls::{Poll, PollOption, PollOptionID, PollVote},
    url::URLMessage,
};

#[test]
fn can_format_html_url() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = URLMessage {
        title: Some("title"),
        summary: Some("summary"),
        url: Some("url"),
        original_url: Some("original_url"),
        item_type: Some("item_type"),
        images: vec!["images"],
        icons: vec!["icons"],
        site_name: Some("site_name"),
        placeholder: false,
    };

    let actual = exporter.format_url(&Config::fake_message(), &balloon);
    let expected = "<a href=\"url\"><div class=\"app_header\"><div class=\"name\">site_name</div></div><div class=\"app_footer\"><div class=\"caption\">title</div><div class=\"subcaption\">summary</div></div></a>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_url_no_lazy() {
    let mut options = Options::fake_options(ExportType::Html);
    options.no_lazy = true;
    let config = Config::fake_app(options);
    let exporter = HTML::new(&config).unwrap();

    let balloon = URLMessage {
        title: Some("title"),
        summary: Some("summary"),
        url: Some("url"),
        original_url: Some("original_url"),
        item_type: Some("item_type"),
        images: vec!["images"],
        icons: vec!["icons"],
        site_name: Some("site_name"),
        placeholder: false,
    };

    let actual = exporter.format_url(&Config::fake_message(), &balloon);
    let expected = "<a href=\"url\"><div class=\"app_header\"><div class=\"name\">site_name</div></div><div class=\"app_footer\"><div class=\"caption\">title</div><div class=\"subcaption\">summary</div></div></a>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_music() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = MusicMessage {
        url: Some("url"),
        preview: Some("preview"),
        artist: Some("artist"),
        album: Some("album"),
        track_name: Some("track_name"),
        lyrics: None,
    };

    let actual = exporter.format_music(&balloon);
    let expected = "<div class=\"app_header\"><div class=\"name\">track_name</div></div><a href=\"url\"><div class=\"app_footer\"><div class=\"caption\">artist</div><div class=\"subcaption\">album</div></div></a>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_music_lyrics() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = MusicMessage {
        url: Some("url"),
        preview: None,
        artist: Some("artist"),
        album: Some("album"),
        track_name: Some("track_name"),
        lyrics: Some(vec!["a", "b"]),
    };

    let actual = exporter.format_music(&balloon);
    let expected = "<div class=\"app_header\"><div class=\"name\">track_name</div><div class=\"ldtext\"><p>a</p><p>b</p></div></div><a href=\"url\"><div class=\"app_footer\"><div class=\"caption\">artist</div><div class=\"subcaption\">album</div></div></a>";

    assert_eq!(actual, expected);
}

#[test]
fn music_balloon_skips_empty_string_fields() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = MusicMessage {
        url: Some("url"),
        preview: None,
        artist: Some(""),
        album: Some(""),
        track_name: Some("track_name"),
        lyrics: None,
    };

    let actual = exporter.format_music(&balloon);
    let expected =
        "<div class=\"app_header\"><div class=\"name\">track_name</div></div><a href=\"url\"></a>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_collaboration() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = CollaborationMessage {
        original_url: Some("original_url"),
        url: Some("url"),
        title: Some("title"),
        creation_date: Some(0.),
        bundle_id: Some("bundle_id"),
        app_name: Some("app_name"),
    };

    let actual = exporter.format_collaboration(&balloon);
    let expected = "<div class=\"app_header\"><div class=\"name\">app_name</div></div><a href=\"url\"><div class=\"app_footer\"><div class=\"caption\">title</div><div class=\"subcaption\">url</div></div></a>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_apple_pay() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = AppMessage {
        image: Some("image"),
        url: Some("url"),
        title: Some("title"),
        subtitle: Some("subtitle"),
        caption: Some("caption"),
        subcaption: Some("subcaption"),
        trailing_caption: Some("trailing_caption"),
        trailing_subcaption: Some("trailing_subcaption"),
        app_name: Some("app_name"),
        ldtext: Some("ldtext"),
    };

    let actual = exporter.format_apple_pay(&balloon);
    let expected = "<div class=\"app_header\">\n    <div class=\"name\">app_name</div>\n</div><div class=\"app_footer\">\n    <div class=\"caption\">ldtext</div>\n</div>";

    assert_eq!(actual, expected);
}

#[test]
fn apple_pay_balloon_emits_nothing_when_both_fields_missing() {
    // Apple Pay balloons with no `app_name` and no `ldtext` must render
    // nothing. `.app_footer` has a grey background + borders in style.css,
    // so an empty wrapper would render as a visible bordered strip.
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = AppMessage {
        image: None,
        url: None,
        title: None,
        subtitle: None,
        caption: None,
        subcaption: None,
        trailing_caption: None,
        trailing_subcaption: None,
        app_name: None,
        ldtext: None,
    };

    let actual = exporter.format_apple_pay(&balloon);
    assert_eq!(actual, "");
}

#[test]
fn can_format_html_fitness() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = AppMessage {
        image: Some("image"),
        url: Some("url"),
        title: Some("title"),
        subtitle: Some("subtitle"),
        caption: Some("caption"),
        subcaption: Some("subcaption"),
        trailing_caption: Some("trailing_caption"),
        trailing_subcaption: Some("trailing_subcaption"),
        // The label only renders when the payload carries no app name.
        app_name: None,
        ldtext: Some("ldtext"),
    };

    let actual = exporter.format_fitness(&balloon);
    let expected = "<a href=\"url\"><div class=\"app_header\"><div class=\"name\">Fitness</div><div class=\"image_title\">title</div><div class=\"image_subtitle\">subtitle</div><div class=\"ldtext\">ldtext</div></div><div class=\"app_footer\"><div class=\"caption\">caption</div><div class=\"subcaption\">subcaption</div><div class=\"trailing_caption\">trailing_caption\n        </div><div class=\"trailing_subcaption\">trailing_subcaption</div></div></a>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_slideshow() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = AppMessage {
        image: Some("image"),
        url: Some("url"),
        title: Some("title"),
        subtitle: Some("subtitle"),
        caption: Some("caption"),
        subcaption: Some("subcaption"),
        trailing_caption: Some("trailing_caption"),
        trailing_subcaption: Some("trailing_subcaption"),
        // The label only renders when the payload carries no app name.
        app_name: None,
        ldtext: Some("ldtext"),
    };

    let actual = exporter.format_slideshow(&balloon);
    let expected = "<a href=\"url\"><div class=\"app_header\"><div class=\"name\">Slideshow</div><div class=\"image_title\">title</div><div class=\"image_subtitle\">subtitle</div><div class=\"ldtext\">ldtext</div></div><div class=\"app_footer\"><div class=\"caption\">caption</div><div class=\"subcaption\">subcaption</div><div class=\"trailing_caption\">trailing_caption\n        </div><div class=\"trailing_subcaption\">trailing_subcaption</div></div></a>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_find_my() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = AppMessage {
        image: Some("image"),
        url: Some("url"),
        title: Some("title"),
        subtitle: Some("subtitle"),
        caption: Some("caption"),
        subcaption: Some("subcaption"),
        trailing_caption: Some("trailing_caption"),
        trailing_subcaption: Some("trailing_subcaption"),
        app_name: Some("app_name"),
        ldtext: Some("ldtext"),
    };

    let actual = exporter.format_find_my(&balloon);
    let expected = "<div class=\"app_header\">\n    <div class=\"name\">app_name</div>\n</div><div class=\"app_footer\">\n    <div class=\"caption\">ldtext</div>\n</div>";

    assert_eq!(actual, expected);
}

#[test]
fn find_my_balloon_emits_nothing_when_both_fields_missing() {
    // An empty Find My payload must not render bare `.app_header` /
    // `.app_footer` wrappers, which would show as a styled grey strip
    // with no content.
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = AppMessage {
        image: None,
        url: None,
        title: None,
        subtitle: None,
        caption: None,
        subcaption: None,
        trailing_caption: None,
        trailing_subcaption: None,
        app_name: None,
        ldtext: None,
    };

    let actual = exporter.format_find_my(&balloon);
    assert_eq!(actual, "");
}

#[test]
fn can_format_html_check_in_timer() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let cases = [
        (
            "timer started",
            "Check\u{a0}In: Timer Started",
            "<div class=\"app_header\">\n    <div class=\"name\">Check&nbsp;In</div><div class=\"ldtext\">Check&nbsp;In: Timer Started</div></div><div class=\"app_footer\">\n    <div class=\"caption\">Checked in at Oct 14, 2023  1:54:29 PM</div>\n</div>",
        ),
        (
            "timer late",
            "Check\u{a0}In: Has not checked in when expected, location shared",
            "<div class=\"app_header\">\n    <div class=\"name\">Check&nbsp;In</div><div class=\"ldtext\">Check&nbsp;In: Has not checked in when expected, location shared</div></div><div class=\"app_footer\">\n    <div class=\"caption\">Checked in at Oct 14, 2023  1:54:29 PM</div>\n</div>",
        ),
        (
            "accepted",
            "Check\u{a0}In: Fake Location",
            "<div class=\"app_header\">\n    <div class=\"name\">Check&nbsp;In</div><div class=\"ldtext\">Check&nbsp;In: Fake Location</div></div><div class=\"app_footer\">\n    <div class=\"caption\">Checked in at Oct 14, 2023  1:54:29 PM</div>\n</div>",
        ),
    ];

    for (label, text, expected) in cases {
        // HTML renders `ldtext`; TXT renders `caption`. Both carry the
        // same text so the rows match across exporters.
        let balloon = AppMessage {
            image: None,
            url: Some("?messageType=1&interfaceVersion=1&sendDate=1697316869.688709"),
            title: None,
            subtitle: None,
            caption: Some(text),
            subcaption: None,
            trailing_caption: None,
            trailing_subcaption: None,
            app_name: Some("Check\u{a0}In"),
            ldtext: Some(text),
        };

        assert_eq!(exporter.format_check_in(&balloon), expected, "{label}");
    }
}

#[test]
fn can_format_html_app_store() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = AppStoreMessage {
        url: Some("url"),
        app_name: Some("app_name"),
        original_url: Some("original_url"),
        description: Some("description"),
        platform: Some("platform"),
        genre: Some("genre"),
    };

    let actual = exporter.format_app_store(&balloon);
    let expected = "<div class=\"app_header\"><div class=\"name\">app_name</div></div><a href=\"url\"><div class=\"app_footer\"><div class=\"caption\">description</div><div class=\"subcaption\">platform</div><div class=\"trailing_subcaption\">genre</div></div></a>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_app_store_no_url_with_original_url() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = AppStoreMessage {
        url: None,
        app_name: Some("app_name"),
        original_url: Some("original_url"),
        description: Some("description"),
        platform: Some("platform"),
        genre: Some("genre"),
    };

    let actual = exporter.format_app_store(&balloon);
    let expected = "<div class=\"app_header\"><div class=\"name\">app_name</div></div><a href=\"original_url\"><div class=\"app_footer\"><div class=\"caption\">description</div><div class=\"subcaption\">platform</div><div class=\"trailing_subcaption\">genre</div></div></a>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_placemark() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = PlacemarkMessage {
        url: Some("url"),
        original_url: Some("original_url"),
        place_name: Some("Name"),
        placemark: Placemark {
            name: Some("name"),
            address: Some("address"),
            state: Some("state"),
            city: Some("city"),
            iso_country_code: Some("iso_country_code"),
            postal_code: Some("postal_code"),
            country: Some("country"),
            street: Some("street"),
            sub_administrative_area: Some("sub_administrative_area"),
            sub_locality: Some("sub_locality"),
        },
    };

    let actual = exporter.format_placemark(&balloon);
    let expected = "<a href=\"url\"><div class=\"app_header\"><div class=\"name\">Name</div><div class=\"image_title\">name</div></div><div class=\"app_footer\"><div class=\"caption\">address</div><div class=\"trailing_caption\">postal_code</div><div class=\"subcaption\">country</div><div class=\"trailing_subcaption\">sub_administrative_area</div></div></a>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_poll() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut poll_options: HashMap<PollOptionID, PollOption> = HashMap::new();

    let id1: PollOptionID = "1".to_string();
    let id2: PollOptionID = "2".to_string();
    let id3: PollOptionID = "3".to_string();

    poll_options.insert(
        id1.clone(),
        PollOption {
            text: "Rust".to_string(),
            creator: "alice".to_string(),
            votes: vec![PollVote {
                voter: "carol".to_string(),
                option_id: id1.clone(),
            }],
        },
    );

    poll_options.insert(
        id2.clone(),
        PollOption {
            text: "Go".to_string(),
            creator: "bob".to_string(),
            votes: vec![
                PollVote {
                    voter: "alice".to_string(),
                    option_id: id2.clone(),
                },
                PollVote {
                    voter: "bob".to_string(),
                    option_id: id2.clone(),
                },
            ],
        },
    );

    poll_options.insert(
        id3.clone(),
        PollOption {
            text: "Python".to_string(),
            creator: "carol".to_string(),
            votes: vec![PollVote {
                voter: "dave".to_string(),
                option_id: id3.clone(),
            }],
        },
    );

    let poll = Poll {
        options: poll_options,
        order: vec![id1, id2, id3],
    };

    let actual = exporter.format_poll(&poll);
    let expected = "<div class=\"poll-container\"><div class=\"poll-option\">\n        <div class=\"option-header\"><span>Rust</span><span class=\"vote-count\">1</span>\n        </div>\n        <div class=\"vote-bar-container\">\n            <div class=\"vote-bar\" style=\"width: 50%;\"></div>\n        </div><div class=\"voters-list\"><span class=\"voter\">carol</span></div></div><div class=\"poll-option\">\n        <div class=\"option-header\"><span>Go</span><span class=\"vote-count\">2</span>\n        </div>\n        <div class=\"vote-bar-container\">\n            <div class=\"vote-bar\" style=\"width: 100%;\"></div>\n        </div><div class=\"voters-list\"><span class=\"voter\">alice</span><span class=\"voter\">bob</span></div></div><div class=\"poll-option\">\n        <div class=\"option-header\"><span>Python</span><span class=\"vote-count\">1</span>\n        </div>\n        <div class=\"vote-bar-container\">\n            <div class=\"vote-bar\" style=\"width: 50%;\"></div>\n        </div><div class=\"voters-list\"><span class=\"voter\">dave</span></div></div></div>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_business_quick_reply_prompt() {
    use imessage_database::message_types::business_chat::{
        BusinessMessage, QuickReply, QuickReplyOption,
    };

    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = BusinessMessage::QuickReply(QuickReply {
        summary: Some("Choose an option".to_string()),
        options: vec![
            QuickReplyOption {
                title: "Yes".to_string(),
            },
            QuickReplyOption {
                title: "No".to_string(),
            },
        ],
        selected_index: None,
    });

    let actual = exporter.format_business(&balloon);
    let expected = "<div class=\"business-balloon\"><div class=\"business-heading\">Choose an option\n    </div>\n    <ul class=\"business-options\"><li\n            class=\"business-option\">Yes</li><li\n            class=\"business-option\">No</li>\n    </ul>\n</div>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_business_quick_reply_selected() {
    use imessage_database::message_types::business_chat::{
        BusinessMessage, QuickReply, QuickReplyOption,
    };

    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = BusinessMessage::QuickReply(QuickReply {
        summary: Some("Replied to a question".to_string()),
        options: vec![
            QuickReplyOption {
                title: "Yes".to_string(),
            },
            QuickReplyOption {
                title: "No".to_string(),
            },
        ],
        selected_index: Some(0),
    });

    let actual = exporter.format_business(&balloon);
    let expected = "<div class=\"business-balloon\"><div class=\"business-heading\">Replied to a question\n    </div>\n    <ul class=\"business-options\"><li\n            class=\"business-option selected\">Yes</li><li\n            class=\"business-option\">No</li>\n    </ul>\n</div>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_business_form_request() {
    use imessage_database::message_types::business_chat::{BusinessMessage, FormRequest};

    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = BusinessMessage::FormRequest(FormRequest {
        title: Some("Report an Issue".to_string()),
        subtitle: Some("Tap to get started".to_string()),
    });

    let actual = exporter.format_business(&balloon);
    let expected = "<div class=\"business-balloon\"><div class=\"business-heading\">Report an Issue</div><div class=\"business-subtitle\">Tap to get started</div></div>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_business_form_response() {
    use imessage_database::message_types::business_chat::{
        BusinessMessage, FormAnswer, FormResponse,
    };

    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = BusinessMessage::FormResponse(FormResponse {
        summary: Some("Here's my completed form".to_string()),
        answers: vec![
            FormAnswer {
                question: "Which option best describes your request?".to_string(),
                answers: vec!["The first example option".to_string()],
            },
            FormAnswer {
                question: "When did this happen?".to_string(),
                answers: vec!["01/01/2024".to_string()],
            },
            FormAnswer {
                question: "Anything else to add?".to_string(),
                answers: vec!["Example free-text response.".to_string()],
            },
        ],
    });

    let actual = exporter.format_business(&balloon);
    let expected = "<div class=\"business-balloon\"><div class=\"business-heading\">Here&apos;s my completed form\n    </div><dl class=\"business-answers\"><dt class=\"business-question\">Which option best describes your request?</dt>\n        <dd class=\"business-answer\">The first example option</dd><dt class=\"business-question\">When did this happen?</dt>\n        <dd class=\"business-answer\">01/01/2024</dd><dt class=\"business-question\">Anything else to add?</dt>\n        <dd class=\"business-answer\">Example free-text response.</dd>\n    </dl>\n</div>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_business_list_picker_prompt() {
    use imessage_database::message_types::business_chat::{
        BusinessMessage, ListPicker, ListPickerItem,
    };

    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = BusinessMessage::ListPicker(ListPicker {
        summary: Some("Select a Product".to_string()),
        items: vec![
            ListPickerItem {
                title: "iPhone".to_string(),
                subtitle: None,
                selected: false,
            },
            ListPickerItem {
                title: "AirPods".to_string(),
                subtitle: Some("Wireless".to_string()),
                selected: false,
            },
            ListPickerItem {
                title: "Apple Watch".to_string(),
                subtitle: None,
                selected: false,
            },
        ],
    });

    let actual = exporter.format_business(&balloon);
    let expected = "<div class=\"business-balloon\"><div class=\"business-heading\">Select a Product\n    </div><ul class=\"business-options\"><li\n            class=\"business-option\">iPhone</li><li\n            class=\"business-option\">AirPods <span class=\"business-option-detail\">Wireless</span></li><li\n            class=\"business-option\">Apple Watch</li></ul>\n</div>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_business_list_picker_reply() {
    use imessage_database::message_types::business_chat::{
        BusinessMessage, ListPicker, ListPickerItem,
    };

    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = BusinessMessage::ListPicker(ListPicker {
        summary: Some("Select a Product".to_string()),
        items: vec![
            ListPickerItem {
                title: "iPhone".to_string(),
                subtitle: None,
                selected: true,
            },
            ListPickerItem {
                title: "AirPods".to_string(),
                subtitle: Some("Wireless".to_string()),
                selected: false,
            },
            ListPickerItem {
                title: "Apple Watch".to_string(),
                subtitle: None,
                selected: false,
            },
        ],
    });

    let actual = exporter.format_business(&balloon);
    let expected = "<div class=\"business-balloon\"><div class=\"business-heading\">Select a Product\n    </div><ul class=\"business-options\"><li\n            class=\"business-option selected\">iPhone</li><li\n            class=\"business-option\">AirPods <span class=\"business-option-detail\">Wireless</span></li><li\n            class=\"business-option\">Apple Watch</li></ul>\n</div>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_generic_app() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = AppMessage {
        image: Some("image"),
        url: Some("url"),
        title: Some("title"),
        subtitle: Some("subtitle"),
        caption: Some("caption"),
        subcaption: Some("subcaption"),
        trailing_caption: Some("trailing_caption"),
        trailing_subcaption: Some("trailing_subcaption"),
        app_name: Some("app_name"),
        ldtext: Some("ldtext"),
    };

    let actual = exporter.format_generic_app(&balloon, "bundle_id", &[], &Config::fake_message());
    let expected = "<a href=\"url\"><div class=\"app_header\"><div class=\"name\">app_name</div><div class=\"image_title\">title</div><div class=\"image_subtitle\">subtitle</div><div class=\"ldtext\">ldtext</div></div><div class=\"app_footer\"><div class=\"caption\">caption</div><div class=\"subcaption\">subcaption</div><div class=\"trailing_caption\">trailing_caption\n        </div><div class=\"trailing_subcaption\">trailing_subcaption</div></div></a>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_digital_touch_kiss() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let payload_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/digital_touch_message/kiss.bin");
    let mut payload = vec![];
    File::open(payload_path)
        .unwrap()
        .read_to_end(&mut payload)
        .unwrap();
    let balloon = DigitalTouchMessage::from_payload(&payload).unwrap();

    let msg = Config::fake_message();
    let actual = exporter.format_digital_touch(&msg, &balloon);

    assert!(actual.starts_with("<div class=\"digital_touch\">"));
    assert!(actual.contains("<svg"));
    assert!(actual.contains("<title>Digital Touch Kiss (1 kiss)</title>"));
    assert!(actual.contains("fill=\"red\""));
}

#[test]
fn can_format_html_digital_touch_from_payload() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let payload_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/digital_touch_message/sketch.bin");
    let mut payload = vec![];
    File::open(payload_path)
        .unwrap()
        .read_to_end(&mut payload)
        .unwrap();
    let touch = DigitalTouchMessage::from_payload(&payload).unwrap();

    let msg = Config::fake_message();
    let actual = exporter.format_digital_touch(&msg, &touch);

    assert!(actual.starts_with("<div class=\"digital_touch\">"));
    assert!(actual.contains("<polyline"));
    assert!(actual.contains("rgba(255, 0, 252, 1)"));
}

#[test]
fn can_format_html_digital_touch_video_without_attachment() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let payload_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/digital_touch_message/video.bin");
    let mut payload = vec![];
    File::open(payload_path)
        .unwrap()
        .read_to_end(&mut payload)
        .unwrap();
    let balloon = DigitalTouchMessage::from_payload(&payload).unwrap();

    let msg = Config::fake_message();
    let actual = exporter.format_digital_touch(&msg, &balloon);

    assert!(actual.starts_with("<div class=\"digital_touch\">"));
    assert!(actual.contains("<title>Digital Touch Video</title>"));
    assert!(actual.contains(">Video</text>"));
}

#[test]
fn can_format_html_handwriting() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let payload_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/handwritten_message/handwriting.bin");
    let mut payload = vec![];
    File::open(payload_path)
        .unwrap()
        .read_to_end(&mut payload)
        .unwrap();
    let balloon = HandwrittenMessage::from_payload(&payload).unwrap();

    let mut expected = String::new();
    let expected_path = current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("imessage-database/test_data/handwritten_message/handwriting.svg");
    File::open(expected_path)
        .unwrap()
        .read_to_string(&mut expected)
        .unwrap();

    let msg = Config::fake_message();
    let actual = exporter.format_handwriting(&msg, &balloon);

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_check_in_estimated_end_time() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = AppMessage {
        image: None,
        url: Some("?messageType=1&interfaceVersion=1&estimatedEndTime=1697316869.688709"),
        title: None,
        subtitle: None,
        caption: Some("Check In: Timer Started"),
        subcaption: None,
        trailing_caption: None,
        trailing_subcaption: None,
        app_name: Some("Check In"),
        ldtext: Some("Check In: Timer Started"),
    };

    let actual = exporter.format_check_in(&balloon);
    let expected = "<div class=\"app_header\">\n    <div class=\"name\">Check In</div><div class=\"ldtext\">Check In: Timer Started</div></div><div class=\"app_footer\">\n    <div class=\"caption\">Expected at Oct 14, 2023  1:54:29 PM</div>\n</div>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_check_in_trigger_time() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = AppMessage {
        image: None,
        url: Some("?messageType=1&interfaceVersion=1&triggerTime=1697316869.688709"),
        title: None,
        subtitle: None,
        caption: Some("Check In: Timer Started"),
        subcaption: None,
        trailing_caption: None,
        trailing_subcaption: None,
        app_name: Some("Check In"),
        ldtext: Some("Check In: Timer Started"),
    };

    let actual = exporter.format_check_in(&balloon);
    let expected = "<div class=\"app_header\">\n    <div class=\"name\">Check In</div><div class=\"ldtext\">Check In: Timer Started</div></div><div class=\"app_footer\">\n    <div class=\"caption\">Was expected at Oct 14, 2023  1:54:29 PM</div>\n</div>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_check_in_no_recognized_metadata() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = AppMessage {
        image: None,
        url: Some("?messageType=1"),
        title: None,
        subtitle: None,
        caption: Some("Check In"),
        subcaption: None,
        trailing_caption: None,
        trailing_subcaption: None,
        app_name: Some("Check In"),
        ldtext: Some("Check In"),
    };

    // Without any of the three recognized timestamp keys the footer is
    // omitted entirely (CheckInVM.footer = None → check_in.html drops the
    // `<div class="app_footer">` block).
    let actual = exporter.format_check_in(&balloon);
    let expected = "<div class=\"app_header\">\n    <div class=\"name\">Check In</div><div class=\"ldtext\">Check In</div></div>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_poll_empty_options() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    // Empty poll: `order` is empty, so max_votes = 0 (the unwrap_or guard
    // protects bar_width's `checked_div(0)` from panicking even though the
    // for-loop never executes).
    let poll = Poll {
        options: HashMap::new(),
        order: vec![],
    };

    let actual = exporter.format_poll(&poll);
    let expected = "<div class=\"poll-container\"></div>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_poll_option_with_zero_votes() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let mut poll_options: HashMap<PollOptionID, PollOption> = HashMap::new();
    let id: PollOptionID = "1".to_string();
    poll_options.insert(
        id.clone(),
        PollOption {
            text: "Rust".to_string(),
            creator: "alice".to_string(),
            votes: vec![],
        },
    );

    let poll = Poll {
        options: poll_options,
        order: vec![id],
    };

    let actual = exporter.format_poll(&poll);
    let expected = "<div class=\"poll-container\"><div class=\"poll-option\">\n        <div class=\"option-header\"><span>Rust</span><span class=\"vote-count\">0</span>\n        </div>\n        <div class=\"vote-bar-container\">\n            <div class=\"vote-bar\" style=\"width: 0%;\"></div>\n        </div></div></div>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_url_no_site_name_falls_back_to_url() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = URLMessage {
        title: None,
        summary: None,
        url: Some("https://example.com"),
        original_url: None,
        item_type: None,
        images: vec![],
        icons: vec![],
        site_name: None,
        placeholder: false,
    };

    // No images → no <img>; no site_name → name falls back to balloon.url.
    // No title or summary → <div class="app_footer"> block is dropped.
    let actual = exporter.format_url(&Config::fake_message(), &balloon);
    let expected = "<a href=\"https://example.com\"><div class=\"app_header\"><div class=\"name\">https://example.com</div></div></a>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_url_no_url_falls_back_to_msg_text() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    let balloon = URLMessage {
        title: None,
        summary: None,
        url: None,
        original_url: None,
        item_type: None,
        images: vec![],
        icons: vec![],
        site_name: None,
        placeholder: false,
    };

    let mut msg = Config::fake_message();
    msg.text = Some("https://example.com/from-text".to_string());

    // No balloon URL → wrapper_url and name both fall back to msg.text.
    let actual = exporter.format_url(&msg, &balloon);
    let expected = "<a href=\"https://example.com/from-text\"><div class=\"app_header\"><div class=\"name\">https://example.com/from-text</div></div></a>";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_html_collaboration_no_url_with_original_url() {
    let config = config(ExportType::Html);
    let exporter = HTML::new(&config).unwrap();

    // wrapper_url is gated on balloon.url; footer_url uses get_url() which
    // falls back to original_url. With url=None + original_url=Some, the
    // <a> wrapper is dropped but the footer subcaption still appears.
    let balloon = CollaborationMessage {
        original_url: Some("https://example.com/original"),
        url: None,
        title: Some("Doc title"),
        creation_date: None,
        bundle_id: Some("bundle"),
        app_name: Some("App"),
    };

    let actual = exporter.format_collaboration(&balloon);
    let expected = "<div class=\"app_header\"><div class=\"name\">App</div></div><div class=\"app_footer\"><div class=\"caption\">Doc title</div><div class=\"subcaption\">https://example.com/original</div></div>";

    assert_eq!(actual, expected);
}
