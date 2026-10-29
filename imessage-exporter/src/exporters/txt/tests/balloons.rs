use std::{collections::HashMap, env::current_dir, fs::File, io::Read};

use crate::{
    Config, TXT,
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
fn can_format_txt_url() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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
    let expected = "url\ntitle\nsummary";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_music() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let balloon = MusicMessage {
        url: Some("url"),
        preview: Some("preview"),
        artist: Some("artist"),
        album: Some("album"),
        track_name: Some("track_name"),
        lyrics: None,
    };

    let actual = exporter.format_music(&balloon);
    let expected = "track_name\nalbum\nartist\nurl";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_music_lyrics() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let balloon = MusicMessage {
        url: Some("url"),
        preview: None,
        artist: Some("artist"),
        album: Some("album"),
        track_name: Some("track_name"),
        lyrics: Some(vec!["a", "b"]),
    };

    let actual = exporter.format_music(&balloon);
    let expected = "Lyrics:\na\nb\n\n\ntrack_name\nalbum\nartist\nurl";

    assert_eq!(actual, expected);
}

#[test]
fn music_balloon_skips_empty_string_fields() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let balloon = MusicMessage {
        url: Some("url"),
        preview: None,
        artist: Some("artist"),
        album: Some(""),
        track_name: Some("track_name"),
        lyrics: None,
    };

    let actual = exporter.format_music(&balloon);
    let expected = "track_name\nartist\nurl";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_collaboration() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let balloon = CollaborationMessage {
        original_url: Some("original_url"),
        url: Some("url"),
        title: Some("title"),
        creation_date: Some(0.),
        bundle_id: Some("bundle_id"),
        app_name: Some("app_name"),
    };

    let actual = exporter.format_collaboration(&balloon);
    let expected = "app_name message:\ntitle\nurl";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_apple_pay() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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
    let expected = "caption transaction: ldtext";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_fitness() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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

    let actual = exporter.format_fitness(&balloon);
    let expected = "app_name message: ldtext";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_slideshow() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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

    let actual = exporter.format_slideshow(&balloon);
    let expected = "Photo album: ldtext url";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_slideshow_url_only_no_leading_space() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let balloon = AppMessage {
        image: None,
        url: Some("url"),
        title: None,
        subtitle: None,
        caption: None,
        subcaption: None,
        trailing_caption: None,
        trailing_subcaption: None,
        app_name: None,
        ldtext: None,
    };

    assert_eq!(exporter.format_slideshow(&balloon), "url");
}

#[test]
fn can_format_txt_slideshow_ldtext_only() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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
        ldtext: Some("ldtext"),
    };

    assert_eq!(exporter.format_slideshow(&balloon), "Photo album: ldtext");
}

#[test]
fn can_format_txt_slideshow_empty_when_both_missing() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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

    assert_eq!(exporter.format_slideshow(&balloon), "");
}

#[test]
fn can_format_txt_find_my() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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
    let expected = "app_name: ldtext";

    assert_eq!(actual, expected);
}

#[test]
fn find_my_balloon_drops_trailing_colon_when_ldtext_missing() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let balloon = AppMessage {
        image: None,
        url: None,
        title: None,
        subtitle: None,
        caption: None,
        subcaption: None,
        trailing_caption: None,
        trailing_subcaption: None,
        app_name: Some("Find My"),
        ldtext: None,
    };

    let actual = exporter.format_find_my(&balloon);
    assert_eq!(actual, "Find My");
}

#[test]
fn can_format_txt_check_in_timer() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let cases = [
        (
            "timer started",
            "Check\u{a0}In: Timer Started",
            "Check\u{a0}In: Timer Started\nChecked in at Oct 14, 2023  1:54:29 PM",
        ),
        (
            "timer late",
            "Check\u{a0}In: Has not checked in when expected, location shared",
            "Check\u{a0}In: Has not checked in when expected, location shared\nChecked in at Oct 14, 2023  1:54:29 PM",
        ),
        (
            "accepted",
            "Check\u{a0}In: Fake Location",
            "Check\u{a0}In: Fake Location\nChecked in at Oct 14, 2023  1:54:29 PM",
        ),
    ];

    for (label, text, expected) in cases {
        // TXT renders `caption`; HTML renders `ldtext`. Both carry the
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
fn can_format_txt_app_store() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let balloon = AppStoreMessage {
        url: Some("url"),
        app_name: Some("app_name"),
        original_url: Some("original_url"),
        description: Some("description"),
        platform: Some("platform"),
        genre: Some("genre"),
    };

    let actual = exporter.format_app_store(&balloon);
    let expected = "app_name\ndescription\nplatform\ngenre\nurl";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_app_store_no_url_with_original_url() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let balloon = AppStoreMessage {
        url: None,
        app_name: Some("app_name"),
        original_url: Some("original_url"),
        description: Some("description"),
        platform: Some("platform"),
        genre: Some("genre"),
    };

    let actual = exporter.format_app_store(&balloon);
    let expected = "app_name\ndescription\nplatform\ngenre\noriginal_url";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_placemark() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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
    let expected = "Name\nurl\nname\naddress\nstate\ncity\niso_country_code\npostal_code\ncountry\nstreet\nsub_administrative_area\nsub_locality";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_poll() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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
    let expected = "- Rust (1)\n  - carol\n- Go (2)\n  - alice\n  - bob\n- Python (1)\n  - dave";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_business_quick_reply_prompt() {
    use imessage_database::message_types::business_chat::{
        BusinessMessage, QuickReply, QuickReplyOption,
    };

    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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
    let expected = "Choose an option\n- Yes\n- No";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_business_quick_reply_selected() {
    use imessage_database::message_types::business_chat::{
        BusinessMessage, QuickReply, QuickReplyOption,
    };

    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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
    let expected = "Replied to a question\n- Yes ✓\n- No";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_business_form_request() {
    use imessage_database::message_types::business_chat::{BusinessMessage, FormRequest};

    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let balloon = BusinessMessage::FormRequest(FormRequest {
        title: Some("Report an Issue".to_string()),
        subtitle: Some("Tap to get started".to_string()),
    });

    let actual = exporter.format_business(&balloon);
    let expected = "Report an Issue\nTap to get started";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_business_form_response() {
    use imessage_database::message_types::business_chat::{
        BusinessMessage, FormAnswer, FormResponse,
    };

    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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
    let expected = "Here's my completed form\n- Which option best describes your request? → The first example option\n- When did this happen? → 01/01/2024\n- Anything else to add? → Example free-text response.";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_business_list_picker_prompt() {
    use imessage_database::message_types::business_chat::{
        BusinessMessage, ListPicker, ListPickerItem,
    };

    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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
    let expected = "Select a Product\n- iPhone\n- AirPods (Wireless)\n- Apple Watch";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_business_list_picker_reply() {
    use imessage_database::message_types::business_chat::{
        BusinessMessage, ListPicker, ListPickerItem,
    };

    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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
    let expected = "Select a Product\n- iPhone ✓\n- AirPods (Wireless)\n- Apple Watch";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_generic_app() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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
    let expected = "app_name message:\ntitle\nsubtitle\ncaption\nsubcaption\ntrailing_caption\ntrailing_subcaption";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_digital_touch_kiss() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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

    assert_eq!(actual, "Digital Touch Kiss (1 kiss)");
}

#[test]
fn can_format_txt_handwriting_disabled_mode() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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
        .join("imessage-database/test_data/handwritten_message/handwriting.ascii");
    File::open(expected_path)
        .unwrap()
        .read_to_string(&mut expected)
        .unwrap();

    let msg = Config::fake_message();
    let actual = exporter.format_handwriting(&msg, &balloon);

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_check_in_estimated_end_time() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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
    let expected = "Check In: Timer Started\nExpected at Oct 14, 2023  1:54:29 PM";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_check_in_trigger_time() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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
    let expected = "Check In: Timer Started\nWas expected at Oct 14, 2023  1:54:29 PM";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_check_in_no_recognized_metadata() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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

    // No timestamp metadata → footer is None → only the caption renders.
    let actual = exporter.format_check_in(&balloon);
    let expected = "Check In";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_poll_empty_options() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let poll = Poll {
        options: HashMap::new(),
        order: vec![],
    };

    // Empty poll: the for-loop body doesn't execute, so the template
    // emits nothing at all.
    let actual = exporter.format_poll(&poll);
    let expected = "";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_poll_option_with_zero_votes() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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
    let expected = "- Rust (0)";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_url_no_url_falls_back_to_msg_text() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

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

    let actual = exporter.format_url(&msg, &balloon);
    let expected = "https://example.com/from-text";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_collaboration_no_url_with_original_url() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    // url=None + original_url=Some: TXT's `url` field is balloon.get_url()
    // which falls back to original_url, so the URL line still renders.
    let balloon = CollaborationMessage {
        original_url: Some("https://example.com/original"),
        url: None,
        title: Some("Doc title"),
        creation_date: None,
        bundle_id: Some("bundle"),
        app_name: Some("App"),
    };

    let actual = exporter.format_collaboration(&balloon);
    let expected = "App message:\nDoc title\nhttps://example.com/original";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_collaboration_drops_label_when_no_app_name_or_bundle_id() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let balloon = CollaborationMessage {
        original_url: None,
        url: Some("https://example.com/doc"),
        title: Some("Doc title"),
        creation_date: None,
        bundle_id: None,
        app_name: None,
    };

    let actual = exporter.format_collaboration(&balloon);
    let expected = "Doc title\nhttps://example.com/doc";

    assert_eq!(actual, expected);
}

#[test]
fn can_format_txt_collaboration_drops_label_when_app_name_is_empty_string() {
    let config = config(ExportType::Txt);
    let exporter = TXT::new(&config).unwrap();

    let balloon = CollaborationMessage {
        original_url: None,
        url: Some("https://example.com/doc"),
        title: Some("Doc title"),
        creation_date: None,
        bundle_id: None,
        app_name: Some(""),
    };

    let actual = exporter.format_collaboration(&balloon);
    let expected = "Doc title\nhttps://example.com/doc";

    assert_eq!(actual, expected);
}
