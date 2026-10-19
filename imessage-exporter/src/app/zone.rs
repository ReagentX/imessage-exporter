/*!
 Timezone for rendering timestamps and setting date-filter bounds.
*/

use std::fmt::{self, Display, Formatter};

use imessage_database::chrono::{
    FixedOffset, Local, MappedLocalTime, NaiveDate, NaiveDateTime, NaiveTime, Offset, TimeZone,
};

/// Timezone selected once in `Options` for rendering and date-filter bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    /// The process timezone: `TZ`, then the system setting.
    Local,
    /// A fixed UTC offset with no daylight-saving rule.
    Fixed(FixedOffset),
}

/// UTC offset and associated [`Zone`] for one instant.
///
/// Keep the zone alongside the offset for reconstruction through `from_offset`.
#[derive(Clone, Copy, Debug)]
pub struct ZoneOffset {
    /// The selected timezone.
    zone: Zone,
    /// The fixed UTC offset for this instant.
    fixed: FixedOffset,
}

impl Offset for ZoneOffset {
    fn fix(&self) -> FixedOffset {
        self.fixed
    }
}

// Format the offset for `Debug` and `Display` on `DateTime<Tz>`.
impl Display for ZoneOffset {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.fixed, f)
    }
}

impl TimeZone for Zone {
    type Offset = ZoneOffset;

    fn from_offset(offset: &ZoneOffset) -> Zone {
        offset.zone
    }

    /// Determine the fixed offset for the given local datetime.
    fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> MappedLocalTime<ZoneOffset> {
        let wrap = |fixed| ZoneOffset { zone: *self, fixed };
        match self {
            Zone::Local => Local.offset_from_local_datetime(local).map(wrap),
            Zone::Fixed(fixed) => MappedLocalTime::Single(wrap(*fixed)),
        }
    }

    /// Determine the fixed offset for the given UTC datetime.
    fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> ZoneOffset {
        let fixed = match self {
            Zone::Local => Local.offset_from_utc_datetime(utc),
            Zone::Fixed(fixed) => *fixed,
        };
        ZoneOffset { zone: *self, fixed }
    }

    /// Required by `TimeZone`
    fn offset_from_local_date(&self, local: &NaiveDate) -> MappedLocalTime<ZoneOffset> {
        self.offset_from_local_datetime(&local.and_time(NaiveTime::MIN))
    }

    /// Required by `TimeZone`
    fn offset_from_utc_date(&self, utc: &NaiveDate) -> ZoneOffset {
        self.offset_from_utc_datetime(&utc.and_time(NaiveTime::MIN))
    }
}

#[cfg(test)]
mod tests {
    use imessage_database::{
        chrono::{FixedOffset, TimeZone, Utc},
        util::dates::format,
    };

    use super::Zone;

    #[test]
    fn fixed_zone_renders_wall_clock() {
        // 2022-05-18 00:29:42 UTC is 17:29:42 the previous day at -07:00.
        let instant = Utc.with_ymd_and_hms(2022, 5, 18, 0, 29, 42).unwrap();
        let zone = Zone::Fixed(FixedOffset::west_opt(7 * 3600).unwrap());
        assert_eq!(
            format(&instant.with_timezone(&zone)),
            "May 17, 2022  5:29:42 PM"
        );
    }

    #[test]
    fn from_offset_round_trips_zone() {
        let zone = Zone::Fixed(FixedOffset::east_opt(5 * 3600 + 30 * 60).unwrap());
        let zoned = Utc
            .with_ymd_and_hms(2022, 5, 18, 0, 29, 42)
            .unwrap()
            .with_timezone(&zone);
        assert_eq!(zoned.timezone(), zone);
        assert_eq!(zoned.offset().to_string(), "+05:30");
    }

    #[test]
    fn fixed_zone_resolves_local_wall_clock_once() {
        // With a fixed offset and no daylight rule, expect exactly one instant
        // for each local wall-clock time.
        let zone = Zone::Fixed(FixedOffset::west_opt(7 * 3600).unwrap());
        let zoned = zone
            .with_ymd_and_hms(2022, 5, 17, 17, 29, 42)
            .single()
            .unwrap();
        assert_eq!(zoned.timestamp(), 1652833782);
    }
}
