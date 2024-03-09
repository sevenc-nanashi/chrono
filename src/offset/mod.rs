// This is a part of Chrono.
// See README.md and LICENSE.txt for details.

//! The time zone, which calculates offsets from the local time to UTC.
//!
//! There are four operations provided by the `TimeZone` trait:
//!
//! 1. Converting the local `NaiveDateTime` to `DateTime<Tz>`
//! 2. Converting the UTC `NaiveDateTime` to `DateTime<Tz>`
//! 3. Converting `DateTime<Tz>` to the local `NaiveDateTime`
//! 4. Constructing `DateTime<Tz>` objects from various offsets
//!
//! 1 is used for constructors. 2 is used for the `with_timezone` method of date and time types.
//! 3 is used for other methods, e.g. `year()` or `format()`, and provided by an associated type
//! which implements `Offset` (which then passed to `TimeZone` for actual implementations).
//! Technically speaking `TimeZone` has a total knowledge about given timescale,
//! but `Offset` is used as a cache to avoid the repeated conversion
//! and provides implementations for 1 and 3.
//! An `TimeZone` instance can be reconstructed from the corresponding `Offset` instance.

use core::fmt;

use crate::format::{parse, ParseResult, Parsed, StrftimeItems};
use crate::naive::{NaiveDate, NaiveDateTime, NaiveTime};
use crate::Weekday;
#[allow(deprecated)]
use crate::{Date, DateTime};

pub(crate) mod fixed;
pub use self::fixed::FixedOffset;

#[cfg(feature = "clock")]
pub(crate) mod local;
#[cfg(feature = "clock")]
pub use self::local::Local;

pub(crate) mod utc;
pub use self::utc::Utc;

/// The conversion result from the local time to the timezone-aware datetime types.
#[derive(Clone, PartialEq, Debug, Copy, Eq, Hash)]
pub enum MappedLocalTime<T> {
    /// Given local time representation is invalid.
    /// This can occur when, for example, the positive timezone transition.
    None,
    /// Given local time representation has a single unique result.
    Single(T),
    /// Given local time representation has multiple results and thus ambiguous.
    /// This can occur when, for example, the negative timezone transition.
    Ambiguous(T /* min */, T /* max */),
}

impl<T> MappedLocalTime<T> {
    /// Returns `Some` only when the conversion result is unique, or `None` otherwise.
    #[must_use]
    pub fn single(self) -> Option<T> {
        match self {
            MappedLocalTime::Single(t) => Some(t),
            _ => None,
        }
    }

    /// Returns `Some` for the earliest possible conversion result, or `None` if none.
    #[must_use]
    pub fn earliest(self) -> Option<T> {
        match self {
            MappedLocalTime::Single(t) | MappedLocalTime::Ambiguous(t, _) => Some(t),
            _ => None,
        }
    }

    /// Returns `Some` for the latest possible conversion result, or `None` if none.
    #[must_use]
    pub fn latest(self) -> Option<T> {
        match self {
            MappedLocalTime::Single(t) | MappedLocalTime::Ambiguous(_, t) => Some(t),
            _ => None,
        }
    }

    /// Maps a `MappedLocalTime<T>` into `MappedLocalTime<U>` with given function.
    #[must_use]
    pub fn map<U, F: FnMut(T) -> U>(self, mut f: F) -> MappedLocalTime<U> {
        match self {
            MappedLocalTime::None => MappedLocalTime::None,
            MappedLocalTime::Single(v) => MappedLocalTime::Single(f(v)),
            MappedLocalTime::Ambiguous(min, max) => MappedLocalTime::Ambiguous(f(min), f(max)),
        }
    }
}

/// The conversion result from the local time to the timezone-aware datetime types.
pub type LocalResult<T> = MappedLocalTime<T>;

#[allow(deprecated)]
impl<Tz: TimeZone> MappedLocalTime<Date<Tz>> {
    /// Makes a new `DateTime` from the current date and given `NaiveTime`.
    /// The offset in the current date is preserved.
    ///
    /// Propagates any error. Ambiguous result would be discarded.
    #[inline]
    #[must_use]
    pub fn and_time(self, time: NaiveTime) -> MappedLocalTime<DateTime<Tz>> {
        match self {
            MappedLocalTime::Single(d) => {
                d.and_time(time).map_or(MappedLocalTime::None, MappedLocalTime::Single)
            }
            _ => MappedLocalTime::None,
        }
    }

    /// Makes a new `DateTime` from the current date, hour, minute and second.
    /// The offset in the current date is preserved.
    ///
    /// Propagates any error. Ambiguous result would be discarded.
    #[inline]
    #[must_use]
    pub fn and_hms_opt(self, hour: u32, min: u32, sec: u32) -> MappedLocalTime<DateTime<Tz>> {
        match self {
            MappedLocalTime::Single(d) => {
                d.and_hms_opt(hour, min, sec).map_or(MappedLocalTime::None, MappedLocalTime::Single)
            }
            _ => MappedLocalTime::None,
        }
    }

    /// Makes a new `DateTime` from the current date, hour, minute, second and millisecond.
    /// The millisecond part can exceed 1,000 in order to represent the leap second.
    /// The offset in the current date is preserved.
    ///
    /// Propagates any error. Ambiguous result would be discarded.
    #[inline]
    #[must_use]
    pub fn and_hms_milli_opt(
        self,
        hour: u32,
        min: u32,
        sec: u32,
        milli: u32,
    ) -> MappedLocalTime<DateTime<Tz>> {
        match self {
            MappedLocalTime::Single(d) => d
                .and_hms_milli_opt(hour, min, sec, milli)
                .map_or(MappedLocalTime::None, MappedLocalTime::Single),
            _ => MappedLocalTime::None,
        }
    }

    /// Makes a new `DateTime` from the current date, hour, minute, second and microsecond.
    /// The microsecond part can exceed 1,000,000 in order to represent the leap second.
    /// The offset in the current date is preserved.
    ///
    /// Propagates any error. Ambiguous result would be discarded.
    #[inline]
    #[must_use]
    pub fn and_hms_micro_opt(
        self,
        hour: u32,
        min: u32,
        sec: u32,
        micro: u32,
    ) -> MappedLocalTime<DateTime<Tz>> {
        match self {
            MappedLocalTime::Single(d) => d
                .and_hms_micro_opt(hour, min, sec, micro)
                .map_or(MappedLocalTime::None, MappedLocalTime::Single),
            _ => MappedLocalTime::None,
        }
    }

    /// Makes a new `DateTime` from the current date, hour, minute, second and nanosecond.
    /// The nanosecond part can exceed 1,000,000,000 in order to represent the leap second.
    /// The offset in the current date is preserved.
    ///
    /// Propagates any error. Ambiguous result would be discarded.
    #[inline]
    #[must_use]
    pub fn and_hms_nano_opt(
        self,
        hour: u32,
        min: u32,
        sec: u32,
        nano: u32,
    ) -> MappedLocalTime<DateTime<Tz>> {
        match self {
            MappedLocalTime::Single(d) => d
                .and_hms_nano_opt(hour, min, sec, nano)
                .map_or(MappedLocalTime::None, MappedLocalTime::Single),
            _ => MappedLocalTime::None,
        }
    }
}

impl<T: fmt::Debug> MappedLocalTime<T> {
    /// Returns the single unique conversion result, or panics accordingly.
    #[must_use]
    #[track_caller]
    pub fn unwrap(self) -> T {
        match self {
            MappedLocalTime::None => panic!("No such local time"),
            MappedLocalTime::Single(t) => t,
            MappedLocalTime::Ambiguous(t1, t2) => {
                panic!("Ambiguous local time, ranging from {:?} to {:?}", t1, t2)
            }
        }
    }
}

/// The offset from the local time to UTC.
pub trait Offset: Sized + Clone + fmt::Debug {
    /// Returns the fixed offset from UTC to the local time stored.
    fn fix(&self) -> FixedOffset;
}

/// The time zone.
///
/// The methods here are the primary constructors for the [`DateTime`] type.
pub trait TimeZone: Sized + Clone {
    /// An associated offset type.
    /// This type is used to store the actual offset in date and time types.
    /// The original `TimeZone` value can be recovered via `TimeZone::from_offset`.
    type Offset: Offset;

    /// Make a new `DateTime` from year, month, day, time components and current time zone.
    ///
    /// This assumes the proleptic Gregorian calendar, with the year 0 being 1 BCE.
    ///
    /// Returns `MappedLocalTime::None` on invalid input data.
    fn with_ymd_and_hms(
        &self,
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        min: u32,
        sec: u32,
    ) -> MappedLocalTime<DateTime<Self>> {
        match NaiveDate::from_ymd_opt(year, month, day).and_then(|d| d.and_hms_opt(hour, min, sec))
        {
            Some(dt) => self.from_local_datetime(&dt),
            None => MappedLocalTime::None,
        }
    }

    /// Makes a new `Date` from year, month, day and the current time zone.
    /// This assumes the proleptic Gregorian calendar, with the year 0 being 1 BCE.
    ///
    /// The time zone normally does not affect the date (unless it is between UTC-24 and UTC+24),
    /// but it will propagate to the `DateTime` values constructed via this date.
    ///
    /// Panics on the out-of-range date, invalid month and/or day.
    #[deprecated(since = "0.4.23", note = "use `with_ymd_and_hms()` instead")]
    #[allow(deprecated)]
    fn ymd(&self, year: i32, month: u32, day: u32) -> Date<Self> {
        self.ymd_opt(year, month, day).unwrap()
    }

    /// Makes a new `Date` from year, month, day and the current time zone.
    /// This assumes the proleptic Gregorian calendar, with the year 0 being 1 BCE.
    ///
    /// The time zone normally does not affect the date (unless it is between UTC-24 and UTC+24),
    /// but it will propagate to the `DateTime` values constructed via this date.
    ///
    /// Returns `None` on the out-of-range date, invalid month and/or day.
    #[deprecated(since = "0.4.23", note = "use `with_ymd_and_hms()` instead")]
    #[allow(deprecated)]
    fn ymd_opt(&self, year: i32, month: u32, day: u32) -> MappedLocalTime<Date<Self>> {
        match NaiveDate::from_ymd_opt(year, month, day) {
            Some(d) => self.from_local_date(&d),
            None => MappedLocalTime::None,
        }
    }

    /// Makes a new `Date` from year, day of year (DOY or "ordinal") and the current time zone.
    /// This assumes the proleptic Gregorian calendar, with the year 0 being 1 BCE.
    ///
    /// The time zone normally does not affect the date (unless it is between UTC-24 and UTC+24),
    /// but it will propagate to the `DateTime` values constructed via this date.
    ///
    /// Panics on the out-of-range date and/or invalid DOY.
    #[deprecated(
        since = "0.4.23",
        note = "use `from_local_datetime()` with a `NaiveDateTime` instead"
    )]
    #[allow(deprecated)]
    fn yo(&self, year: i32, ordinal: u32) -> Date<Self> {
        self.yo_opt(year, ordinal).unwrap()
    }

    /// Makes a new `Date` from year, day of year (DOY or "ordinal") and the current time zone.
    /// This assumes the proleptic Gregorian calendar, with the year 0 being 1 BCE.
    ///
    /// The time zone normally does not affect the date (unless it is between UTC-24 and UTC+24),
    /// but it will propagate to the `DateTime` values constructed via this date.
    ///
    /// Returns `None` on the out-of-range date and/or invalid DOY.
    #[deprecated(
        since = "0.4.23",
        note = "use `from_local_datetime()` with a `NaiveDateTime` instead"
    )]
    #[allow(deprecated)]
    fn yo_opt(&self, year: i32, ordinal: u32) -> MappedLocalTime<Date<Self>> {
        match NaiveDate::from_yo_opt(year, ordinal) {
            Some(d) => self.from_local_date(&d),
            None => MappedLocalTime::None,
        }
    }

    /// Makes a new `Date` from ISO week date (year and week number), day of the week (DOW) and
    /// the current time zone.
    /// This assumes the proleptic Gregorian calendar, with the year 0 being 1 BCE.
    /// The resulting `Date` may have a different year from the input year.
    ///
    /// The time zone normally does not affect the date (unless it is between UTC-24 and UTC+24),
    /// but it will propagate to the `DateTime` values constructed via this date.
    ///
    /// Panics on the out-of-range date and/or invalid week number.
    #[deprecated(
        since = "0.4.23",
        note = "use `from_local_datetime()` with a `NaiveDateTime` instead"
    )]
    #[allow(deprecated)]
    fn isoywd(&self, year: i32, week: u32, weekday: Weekday) -> Date<Self> {
        self.isoywd_opt(year, week, weekday).unwrap()
    }

    /// Makes a new `Date` from ISO week date (year and week number), day of the week (DOW) and
    /// the current time zone.
    /// This assumes the proleptic Gregorian calendar, with the year 0 being 1 BCE.
    /// The resulting `Date` may have a different year from the input year.
    ///
    /// The time zone normally does not affect the date (unless it is between UTC-24 and UTC+24),
    /// but it will propagate to the `DateTime` values constructed via this date.
    ///
    /// Returns `None` on the out-of-range date and/or invalid week number.
    #[deprecated(
        since = "0.4.23",
        note = "use `from_local_datetime()` with a `NaiveDateTime` instead"
    )]
    #[allow(deprecated)]
    fn isoywd_opt(&self, year: i32, week: u32, weekday: Weekday) -> MappedLocalTime<Date<Self>> {
        match NaiveDate::from_isoywd_opt(year, week, weekday) {
            Some(d) => self.from_local_date(&d),
            None => MappedLocalTime::None,
        }
    }

    /// Makes a new `DateTime` from the number of non-leap seconds
    /// since January 1, 1970 0:00:00 UTC (aka "UNIX timestamp")
    /// and the number of nanoseconds since the last whole non-leap second.
    ///
    /// The nanosecond part can exceed 1,000,000,000 in order to represent a
    /// [leap second](crate::NaiveTime#leap-second-handling), but only when `secs % 60 == 59`.
    /// (The true "UNIX timestamp" cannot represent a leap second unambiguously.)
    ///
    /// # Panics
    ///
    /// Panics on the out-of-range number of seconds and/or invalid nanosecond,
    /// for a non-panicking version see [`timestamp_opt`](#method.timestamp_opt).
    #[deprecated(since = "0.4.23", note = "use `timestamp_opt()` instead")]
    fn timestamp(&self, secs: i64, nsecs: u32) -> DateTime<Self> {
        self.timestamp_opt(secs, nsecs).unwrap()
    }

    /// Makes a new `DateTime` from the number of non-leap seconds
    /// since January 1, 1970 0:00:00 UTC (aka "UNIX timestamp")
    /// and the number of nanoseconds since the last whole non-leap second.
    ///
    /// The nanosecond part can exceed 1,000,000,000 in order to represent a
    /// [leap second](crate::NaiveTime#leap-second-handling), but only when `secs % 60 == 59`.
    /// (The true "UNIX timestamp" cannot represent a leap second unambiguously.)
    ///
    /// # Errors
    ///
    /// Returns `MappedLocalTime::None` on out-of-range number of seconds and/or
    /// invalid nanosecond, otherwise always returns `MappedLocalTime::Single`.
    ///
    /// # Example
    ///
    /// ```
    /// use chrono::{TimeZone, Utc};
    ///
    /// assert_eq!(Utc.timestamp_opt(1431648000, 0).unwrap().to_string(), "2015-05-15 00:00:00 UTC");
    /// ```
    fn timestamp_opt(&self, secs: i64, nsecs: u32) -> MappedLocalTime<DateTime<Self>> {
        match DateTime::from_timestamp(secs, nsecs) {
            Some(dt) => MappedLocalTime::Single(self.from_utc_datetime(&dt.naive_utc())),
            None => MappedLocalTime::None,
        }
    }

    /// Makes a new `DateTime` from the number of non-leap milliseconds
    /// since January 1, 1970 0:00:00 UTC (aka "UNIX timestamp").
    ///
    /// Panics on out-of-range number of milliseconds for a non-panicking
    /// version see [`timestamp_millis_opt`](#method.timestamp_millis_opt).
    #[deprecated(since = "0.4.23", note = "use `timestamp_millis_opt()` instead")]
    fn timestamp_millis(&self, millis: i64) -> DateTime<Self> {
        self.timestamp_millis_opt(millis).unwrap()
    }

    /// Makes a new `DateTime` from the number of non-leap milliseconds
    /// since January 1, 1970 0:00:00 UTC (aka "UNIX timestamp").
    ///
    ///
    /// Returns `MappedLocalTime::None` on out-of-range number of milliseconds
    /// and/or invalid nanosecond, otherwise always returns
    /// `MappedLocalTime::Single`.
    ///
    /// # Example
    ///
    /// ```
    /// use chrono::{MappedLocalTime, TimeZone, Utc};
    /// match Utc.timestamp_millis_opt(1431648000) {
    ///     MappedLocalTime::Single(dt) => assert_eq!(dt.timestamp(), 1431648),
    ///     _ => panic!("Incorrect timestamp_millis"),
    /// };
    /// ```
    fn timestamp_millis_opt(&self, millis: i64) -> MappedLocalTime<DateTime<Self>> {
        match DateTime::from_timestamp_millis(millis) {
            Some(dt) => MappedLocalTime::Single(self.from_utc_datetime(&dt.naive_utc())),
            None => MappedLocalTime::None,
        }
    }

    /// Makes a new `DateTime` from the number of non-leap nanoseconds
    /// since January 1, 1970 0:00:00 UTC (aka "UNIX timestamp").
    ///
    /// Unlike [`timestamp_millis_opt`](#method.timestamp_millis_opt), this never fails.
    ///
    /// # Example
    ///
    /// ```
    /// use chrono::{TimeZone, Utc};
    ///
    /// assert_eq!(Utc.timestamp_nanos(1431648000000000).timestamp(), 1431648);
    /// ```
    fn timestamp_nanos(&self, nanos: i64) -> DateTime<Self> {
        self.from_utc_datetime(&DateTime::from_timestamp_nanos(nanos).naive_utc())
    }

    /// Makes a new `DateTime` from the number of non-leap microseconds
    /// since January 1, 1970 0:00:00 UTC (aka "UNIX timestamp").
    ///
    /// # Example
    ///
    /// ```
    /// use chrono::{TimeZone, Utc};
    ///
    /// assert_eq!(Utc.timestamp_micros(1431648000000).unwrap().timestamp(), 1431648);
    /// ```
    fn timestamp_micros(&self, micros: i64) -> MappedLocalTime<DateTime<Self>> {
        match DateTime::from_timestamp_micros(micros) {
            Some(dt) => MappedLocalTime::Single(self.from_utc_datetime(&dt.naive_utc())),
            None => MappedLocalTime::None,
        }
    }

    /// Parses a string with the specified format string and returns a
    /// `DateTime` with the current offset.
    ///
    /// See the [`crate::format::strftime`] module on the
    /// supported escape sequences.
    ///
    /// If the to-be-parsed string includes an offset, it *must* match the
    /// offset of the TimeZone, otherwise an error will be returned.
    ///
    /// See also [`DateTime::parse_from_str`] which gives a [`DateTime`] with
    /// parsed [`FixedOffset`].
    ///
    /// See also [`NaiveDateTime::parse_from_str`] which gives a [`NaiveDateTime`] without
    /// an offset, but can be converted to a [`DateTime`] with [`NaiveDateTime::and_utc`] or
    /// [`NaiveDateTime::and_local_timezone`].
    #[deprecated(
        since = "0.4.29",
        note = "use `DateTime::parse_from_str` or `NaiveDateTime::parse_from_str` with `and_utc()` or `and_local_timezone()` instead"
    )]
    fn datetime_from_str(&self, s: &str, fmt: &str) -> ParseResult<DateTime<Self>> {
        let mut parsed = Parsed::new();
        parse(&mut parsed, s, StrftimeItems::new(fmt))?;
        parsed.to_datetime_with_timezone(self)
    }

    /// Reconstructs the time zone from the offset.
    fn from_offset(offset: &Self::Offset) -> Self;

    /// Creates the offset(s) for given local `NaiveDate` if possible.
    fn offset_from_local_date(&self, local: &NaiveDate) -> MappedLocalTime<Self::Offset>;

    /// Creates the offset(s) for given local `NaiveDateTime` if possible.
    fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> MappedLocalTime<Self::Offset>;

    /// Converts the local `NaiveDate` to the timezone-aware `Date` if possible.
    #[allow(clippy::wrong_self_convention)]
    #[deprecated(since = "0.4.23", note = "use `from_local_datetime()` instead")]
    #[allow(deprecated)]
    fn from_local_date(&self, local: &NaiveDate) -> MappedLocalTime<Date<Self>> {
        self.offset_from_local_date(local).map(|offset| {
            // since FixedOffset is within +/- 1 day, the date is never affected
            Date::from_utc(*local, offset)
        })
    }

    /// Converts the local `NaiveDateTime` to the timezone-aware `DateTime` if possible.
    #[allow(clippy::wrong_self_convention)]
    fn from_local_datetime(&self, local: &NaiveDateTime) -> MappedLocalTime<DateTime<Self>> {
        // Return `MappedLocalTime::None` when the offset pushes a value out of range, instead of
        // panicking.
        match self.offset_from_local_datetime(local) {
            MappedLocalTime::None => MappedLocalTime::None,
            MappedLocalTime::Single(offset) => match local.checked_sub_offset(offset.fix()) {
                Some(dt) => {
                    MappedLocalTime::Single(DateTime::from_naive_utc_and_offset(dt, offset))
                }
                None => MappedLocalTime::None,
            },
            MappedLocalTime::Ambiguous(o1, o2) => {
                match (local.checked_sub_offset(o1.fix()), local.checked_sub_offset(o2.fix())) {
                    (Some(d1), Some(d2)) => MappedLocalTime::Ambiguous(
                        DateTime::from_naive_utc_and_offset(d1, o1),
                        DateTime::from_naive_utc_and_offset(d2, o2),
                    ),
                    _ => MappedLocalTime::None,
                }
            }
        }
    }

    /// Creates the offset for given UTC `NaiveDate`. This cannot fail.
    fn offset_from_utc_date(&self, utc: &NaiveDate) -> Self::Offset;

    /// Creates the offset for given UTC `NaiveDateTime`. This cannot fail.
    fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> Self::Offset;

    /// Converts the UTC `NaiveDate` to the local time.
    /// The UTC is continuous and thus this cannot fail (but can give the duplicate local time).
    #[allow(clippy::wrong_self_convention)]
    #[deprecated(since = "0.4.23", note = "use `from_utc_datetime()` instead")]
    #[allow(deprecated)]
    fn from_utc_date(&self, utc: &NaiveDate) -> Date<Self> {
        Date::from_utc(*utc, self.offset_from_utc_date(utc))
    }

    /// Converts the UTC `NaiveDateTime` to the local time.
    /// The UTC is continuous and thus this cannot fail (but can give the duplicate local time).
    #[allow(clippy::wrong_self_convention)]
    fn from_utc_datetime(&self, utc: &NaiveDateTime) -> DateTime<Self> {
        DateTime::from_naive_utc_and_offset(*utc, self.offset_from_utc_datetime(utc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_offset_min_max_dates() {
        for offset_hour in -23..=23 {
            dbg!(offset_hour);
            let offset = FixedOffset::east_opt(offset_hour * 60 * 60).unwrap();

            let local_max = offset.from_utc_datetime(&NaiveDateTime::MAX);
            assert_eq!(local_max.naive_utc(), NaiveDateTime::MAX);
            let local_min = offset.from_utc_datetime(&NaiveDateTime::MIN);
            assert_eq!(local_min.naive_utc(), NaiveDateTime::MIN);

            let local_max = offset.from_local_datetime(&NaiveDateTime::MAX);
            if offset_hour >= 0 {
                assert_eq!(local_max.unwrap().naive_local(), NaiveDateTime::MAX);
            } else {
                assert_eq!(local_max, MappedLocalTime::None);
            }
            let local_min = offset.from_local_datetime(&NaiveDateTime::MIN);
            if offset_hour <= 0 {
                assert_eq!(local_min.unwrap().naive_local(), NaiveDateTime::MIN);
            } else {
                assert_eq!(local_min, MappedLocalTime::None);
            }
        }
    }

    #[test]
    fn test_negative_millis() {
        let dt = Utc.timestamp_millis_opt(-1000).unwrap();
        assert_eq!(dt.to_string(), "1969-12-31 23:59:59 UTC");
        let dt = Utc.timestamp_millis_opt(-7000).unwrap();
        assert_eq!(dt.to_string(), "1969-12-31 23:59:53 UTC");
        let dt = Utc.timestamp_millis_opt(-7001).unwrap();
        assert_eq!(dt.to_string(), "1969-12-31 23:59:52.999 UTC");
        let dt = Utc.timestamp_millis_opt(-7003).unwrap();
        assert_eq!(dt.to_string(), "1969-12-31 23:59:52.997 UTC");
        let dt = Utc.timestamp_millis_opt(-999).unwrap();
        assert_eq!(dt.to_string(), "1969-12-31 23:59:59.001 UTC");
        let dt = Utc.timestamp_millis_opt(-1).unwrap();
        assert_eq!(dt.to_string(), "1969-12-31 23:59:59.999 UTC");
        let dt = Utc.timestamp_millis_opt(-60000).unwrap();
        assert_eq!(dt.to_string(), "1969-12-31 23:59:00 UTC");
        let dt = Utc.timestamp_millis_opt(-3600000).unwrap();
        assert_eq!(dt.to_string(), "1969-12-31 23:00:00 UTC");

        for (millis, expected) in &[
            (-7000, "1969-12-31 23:59:53 UTC"),
            (-7001, "1969-12-31 23:59:52.999 UTC"),
            (-7003, "1969-12-31 23:59:52.997 UTC"),
        ] {
            match Utc.timestamp_millis_opt(*millis) {
                MappedLocalTime::Single(dt) => {
                    assert_eq!(dt.to_string(), *expected);
                }
                e => panic!("Got {:?} instead of an okay answer", e),
            }
        }
    }

    #[test]
    fn test_negative_nanos() {
        let dt = Utc.timestamp_nanos(-1_000_000_000);
        assert_eq!(dt.to_string(), "1969-12-31 23:59:59 UTC");
        let dt = Utc.timestamp_nanos(-999_999_999);
        assert_eq!(dt.to_string(), "1969-12-31 23:59:59.000000001 UTC");
        let dt = Utc.timestamp_nanos(-1);
        assert_eq!(dt.to_string(), "1969-12-31 23:59:59.999999999 UTC");
        let dt = Utc.timestamp_nanos(-60_000_000_000);
        assert_eq!(dt.to_string(), "1969-12-31 23:59:00 UTC");
        let dt = Utc.timestamp_nanos(-3_600_000_000_000);
        assert_eq!(dt.to_string(), "1969-12-31 23:00:00 UTC");
    }

    #[test]
    fn test_nanos_never_panics() {
        Utc.timestamp_nanos(i64::max_value());
        Utc.timestamp_nanos(i64::default());
        Utc.timestamp_nanos(i64::min_value());
    }

    #[test]
    fn test_negative_micros() {
        let dt = Utc.timestamp_micros(-1_000_000).unwrap();
        assert_eq!(dt.to_string(), "1969-12-31 23:59:59 UTC");
        let dt = Utc.timestamp_micros(-999_999).unwrap();
        assert_eq!(dt.to_string(), "1969-12-31 23:59:59.000001 UTC");
        let dt = Utc.timestamp_micros(-1).unwrap();
        assert_eq!(dt.to_string(), "1969-12-31 23:59:59.999999 UTC");
        let dt = Utc.timestamp_micros(-60_000_000).unwrap();
        assert_eq!(dt.to_string(), "1969-12-31 23:59:00 UTC");
        let dt = Utc.timestamp_micros(-3_600_000_000).unwrap();
        assert_eq!(dt.to_string(), "1969-12-31 23:00:00 UTC");
    }
}
