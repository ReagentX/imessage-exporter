/*!
 These are the link previews that iMessage generates when sending locations or points of interest from the Maps app.
*/

use plist::Value;

use crate::{
    error::plist::PlistParseError,
    message_types::variants::BalloonProvider,
    util::plist::{get_string_from_dict, get_string_from_nested_dict},
};

/// Representation of Apple's [`CLPlacemark`](https://developer.apple.com/documentation/corelocation/clplacemark) object
#[derive(Debug, PartialEq, Eq, Default)]
pub struct Placemark<'a> {
    /// The name of the placemark
    pub name: Option<&'a str>,
    /// The full address formatted associated with the placemark
    pub address: Option<&'a str>,
    /// The state or province associated with the placemark
    pub state: Option<&'a str>,
    /// The city associated with the placemark
    pub city: Option<&'a str>,
    /// The abbreviated country or region name
    pub iso_country_code: Option<&'a str>,
    /// The postal code associated with the placemark
    pub postal_code: Option<&'a str>,
    /// The name of the country or region associated with the placemark
    pub country: Option<&'a str>,
    /// The street associated with the placemark
    pub street: Option<&'a str>,
    /// Additional administrative area information for the placemark
    pub sub_administrative_area: Option<&'a str>,
    /// Additional city-level information for the placemark
    pub sub_locality: Option<&'a str>,
}

impl<'a> Placemark<'a> {
    /// Create a Placemark from a `specialization2` payload
    fn new(payload: &'a Value) -> Result<Self, PlistParseError> {
        // Parse out the address components dict
        let address_components = payload
            .as_dictionary()
            .ok_or_else(|| {
                PlistParseError::InvalidType(
                    "specialization2".to_string(),
                    "dictionary".to_string(),
                )
            })?
            .get("addressComponents")
            .ok_or_else(|| PlistParseError::MissingKey("addressComponents".to_string()))?;
        Ok(Self {
            name: get_string_from_dict(payload, "name"),
            address: get_string_from_dict(payload, "address"),
            state: get_string_from_dict(address_components, "_state"),
            city: get_string_from_dict(address_components, "_city"),
            iso_country_code: get_string_from_dict(address_components, "_ISOCountryCode"),
            postal_code: get_string_from_dict(address_components, "_postalCode"),
            country: get_string_from_dict(address_components, "_country"),
            street: get_string_from_dict(address_components, "_street"),
            sub_administrative_area: get_string_from_dict(
                address_components,
                "_subAdministrativeArea",
            ),
            sub_locality: get_string_from_dict(address_components, "_subLocality"),
        })
    }
}

/// This struct is not documented by Apple, but represents messages displayed as
/// `com.apple.messages.URLBalloonProvider` but for the Maps app
#[derive(Debug, PartialEq, Eq)]
pub struct PlacemarkMessage<'a> {
    /// The URL that ended up serving content, after all redirects
    pub url: Option<&'a str>,
    /// The original url, before any redirects
    pub original_url: Option<&'a str>,
    /// The full street address of the location
    pub place_name: Option<&'a str>,
    /// [Placemark] data for the specified location
    pub placemark: Placemark<'a>,
}

impl<'a> BalloonProvider<'a> for PlacemarkMessage<'a> {
    fn from_map(payload: &'a Value) -> Result<Self, PlistParseError> {
        if let Ok((placemark, body)) = PlacemarkMessage::get_body_and_url(payload) {
            // Ensure the message is a placemark
            if get_string_from_dict(placemark, "address").is_none() {
                return Err(PlistParseError::WrongMessageType);
            }

            return Ok(Self {
                url: get_string_from_nested_dict(body, "URL"),
                original_url: get_string_from_nested_dict(body, "originalURL"),
                place_name: get_string_from_dict(body, "title"),
                placemark: Placemark::new(placemark).unwrap_or_default(),
            });
        }
        Err(PlistParseError::NoPayload)
    }
}

impl<'a> PlacemarkMessage<'a> {
    /// Extract the main dictionary of data from the body of the payload
    ///
    /// Placemark messages store the URL under `richLinkMetadata` like a normal URL, but has some
    /// extra data stored under `specialization2` that contains the placemark's metadata.
    fn get_body_and_url(payload: &'a Value) -> Result<(&'a Value, &'a Value), PlistParseError> {
        let base = payload
            .as_dictionary()
            .ok_or_else(|| {
                PlistParseError::InvalidType("root".to_string(), "dictionary".to_string())
            })?
            .get("richLinkMetadata")
            .ok_or_else(|| PlistParseError::MissingKey("richLinkMetadata".to_string()))?;
        Ok((
            base.as_dictionary()
                .ok_or_else(|| {
                    PlistParseError::InvalidType("root".to_string(), "dictionary".to_string())
                })?
                .get("specialization2")
                .ok_or_else(|| PlistParseError::MissingKey("specialization2".to_string()))?,
            base,
        ))
    }

    /// Get the redirected URL from a URL message, falling back to the original URL, if it exists
    #[must_use]
    pub fn get_url(&self) -> Option<&str> {
        self.url.or(self.original_url)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        message_types::{
            placemark::{Placemark, PlacemarkMessage},
            variants::BalloonProvider,
        },
        util::plist::parse_ns_keyed_archiver,
    };
    use plist::Value;
    use std::env::current_dir;
    use std::fs::File;

    #[test]
    fn test_parse_app_store_link() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/shared_placemark/SharedPlacemark.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let balloon = PlacemarkMessage::from_map(&parsed).unwrap();
        let expected = PlacemarkMessage {
            url: Some(
                "https://maps.apple.com/?address=Cherry%20Cove,%20Avalon,%20CA%20%2090704,%20United%20States&ll=33.450858,-118.508212&q=Cherry%20Cove&t=m",
            ),
            original_url: Some(
                "https://maps.apple.com/?address=Cherry%20Cove,%20Avalon,%20CA%20%2090704,%20United%20States&ll=33.450858,-118.508212&q=Cherry%20Cove&t=m",
            ),
            place_name: Some("Cherry Cove Avalon CA 90704 United States"),
            placemark: Placemark {
                name: Some("Cherry Cove"),
                address: Some("Cherry Cove, Avalon"),
                state: Some("CA"),
                city: Some("Avalon"),
                iso_country_code: Some("US"),
                postal_code: Some("90704"),
                country: Some("United States"),
                street: Some("Cherry Cove"),
                sub_administrative_area: Some("Los Angeles County"),
                sub_locality: Some("Santa Catalina Island"),
            },
        };

        assert_eq!(balloon, expected);
    }

    #[test]
    fn can_parse_placemark() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/shared_placemark/SharedPlacemark.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();
        let parsed = parse_ns_keyed_archiver(&plist).unwrap();

        let (placemark_data, _) = PlacemarkMessage::get_body_and_url(&parsed).unwrap();

        let placemark = Placemark::new(placemark_data).unwrap();
        let expected = Placemark {
            name: Some("Cherry Cove"),
            address: Some("Cherry Cove, Avalon"),
            state: Some("CA"),
            city: Some("Avalon"),
            iso_country_code: Some("US"),
            postal_code: Some("90704"),
            country: Some("United States"),
            street: Some("Cherry Cove"),
            sub_administrative_area: Some("Los Angeles County"),
            sub_locality: Some("Santa Catalina Island"),
        };

        assert_eq!(placemark, expected);
    }
}
