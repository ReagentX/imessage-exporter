/*!
 Classification for Apple Business Chat payloads.

 The business extension uses one balloon bundle ID for several payload schemas.
 The bundle ID gets us into this module; the decoded payload determines the
 concrete renderer.
*/

use std::str::from_utf8;

use jzon::parse;
use plist::Value;

use crate::{
    error::plist::PlistParseError,
    message_types::{
        business_chat::{
            DYNAMIC_KEY, LIST_PICKER_KEY, QUICK_REPLY_KEY, SELECTIONS_KEY,
            form::{FormRequest, FormResponse},
            list_picker::ListPicker,
            quick_reply::QuickReply,
        },
        variants::BalloonProvider,
    },
    util::plist::get_data_from_dict,
};

/// Parsed business payload supported by the exporters.
#[derive(Debug, PartialEq, Eq)]
pub enum BusinessMessage {
    /// [`QuickReply`] prompt, optionally with the selected option on replies.
    QuickReply(QuickReply),
    /// [`FormRequest`] asking the user to fill out an interactive form.
    FormRequest(FormRequest),
    /// [`FormResponse`] with the answers the user gave.
    FormResponse(FormResponse),
    /// [`ListPicker`] prompt or reply, with selected items marked on replies.
    ListPicker(ListPicker),
}

impl<'a> BalloonProvider<'a> for BusinessMessage {
    /// Classify a resolved business `NSKeyedArchiver` payload.
    ///
    /// Decodes the embedded JSON once and dispatches on its structure. Returns
    /// [`PlistParseError::WrongMessageType`] when the payload carries no
    /// supported business schema, but the source data may still be rendered generically.
    fn from_map(payload: &'a Value) -> Result<Self, PlistParseError> {
        let data = get_data_from_dict(payload, "data").ok_or(PlistParseError::WrongMessageType)?;
        let text = from_utf8(data).map_err(|_| PlistParseError::WrongMessageType)?;
        let json = parse(text).map_err(|_| PlistParseError::WrongMessageType)?;

        if json[QUICK_REPLY_KEY]["items"].is_array() {
            Ok(Self::QuickReply(QuickReply::from_json(&json, payload)))
        } else if json[LIST_PICKER_KEY]["sections"].is_array() {
            Ok(Self::ListPicker(ListPicker::from_json(&json, payload)))
        } else if json[DYNAMIC_KEY].is_object() {
            // Responses carry submitted answers under `dynamic.selections`;
            // requests leave it null.
            if json[DYNAMIC_KEY][SELECTIONS_KEY]
                .as_array()
                .is_some_and(|selections| !selections.is_empty())
            {
                Ok(Self::FormResponse(FormResponse::from_json(&json, payload)))
            } else {
                Ok(Self::FormRequest(FormRequest::from_json(&json, payload)))
            }
        } else {
            Err(PlistParseError::WrongMessageType)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{env::current_dir, fs::File};

    use plist::Value;

    use crate::{
        error::plist::PlistParseError,
        message_types::{business_chat::BusinessMessage, variants::BalloonProvider},
        util::plist::parse_ns_keyed_archiver,
    };

    fn classify(filename: &str) -> Result<BusinessMessage, PlistParseError> {
        let path = current_dir()
            .unwrap()
            .join("test_data/app_message")
            .join(filename);
        let plist = Value::from_reader(File::open(path).unwrap()).unwrap();
        BusinessMessage::from_map(&parse_ns_keyed_archiver(&plist).unwrap())
    }

    /// Build a resolved business payload wrapping `json` in the `data` field,
    /// matching the shape [`parse_ns_keyed_archiver`] produces.
    fn business_payload(json: &str) -> Value {
        let mut dict = plist::Dictionary::new();
        dict.insert("data".to_string(), Value::Data(json.as_bytes().to_vec()));
        Value::Dictionary(dict)
    }

    #[test]
    fn legacy_business_falls_through() {
        // Legacy query-string payloads have no supported business schema.
        assert!(matches!(
            classify("Business.plist"),
            Err(PlistParseError::WrongMessageType)
        ));
    }

    #[test]
    fn empty_form_selections_classify_as_request() {
        // `dynamic` with an empty `selections` array is a blank request, not a
        // response carrying submitted answers.
        assert!(matches!(
            BusinessMessage::from_map(&business_payload(r#"{"dynamic": {"selections": []}}"#)),
            Ok(BusinessMessage::FormRequest(_))
        ));
    }

    #[test]
    fn value_text_does_not_spoof_classification() {
        // A quick reply whose option title contains the other schemas' marker
        // words still classifies structurally as a quick reply.
        assert!(matches!(
            BusinessMessage::from_map(&business_payload(
                r#"{"quick-reply": {"items": [{"title": "listPicker dynamic selections"}]}}"#
            )),
            Ok(BusinessMessage::QuickReply(_))
        ));
    }
}
