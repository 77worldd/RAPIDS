//! The machine's current UTC offset for start windows. The host only accepts
//! a number of minutes, so no operating-system types cross that boundary.

#[cfg(windows)]
mod windows {
    use std::io;
    use std::mem::MaybeUninit;

    // The Windows TIME_ZONE_INFORMATION layout: names have 32 UTF-16 code
    // units, and SYSTEMTIME is eight consecutive 16-bit fields. Bias values
    // are minutes west of UTC, the inverse of the host Clock convention.
    #[repr(C)]
    struct TimeZoneInformation {
        bias: i32,
        _standard_name: [u16; 32],
        _standard_date: [u16; 8],
        standard_bias: i32,
        _daylight_name: [u16; 32],
        _daylight_date: [u16; 8],
        daylight_bias: i32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "GetTimeZoneInformation"]
        fn get_time_zone_information(info: *mut TimeZoneInformation) -> u32;
    }

    fn east_of_utc(bias: i32, seasonal_bias: i32) -> io::Result<i32> {
        bias.checked_add(seasonal_bias)
            .and_then(i32::checked_neg)
            .ok_or_else(|| io::Error::other("the machine's UTC offset is out of range"))
    }

    pub fn local_offset_minutes() -> io::Result<i32> {
        let mut info = MaybeUninit::<TimeZoneInformation>::uninit();
        // SAFETY: the pointer is writable, correctly aligned, and has the
        // complete layout Windows writes on every non-error return.
        let season = unsafe { get_time_zone_information(info.as_mut_ptr()) };
        if season == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a non-error return from GetTimeZoneInformation initialized
        // every field, including both names and transition dates.
        let info = unsafe { info.assume_init() };
        let seasonal_bias = match season {
            0 => 0, // no seasonal change
            1 => info.standard_bias,
            2 => info.daylight_bias,
            _ => return Err(io::Error::other("unknown Windows time-zone state")),
        };
        east_of_utc(info.bias, seasonal_bias)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn west_of_utc_bias_becomes_an_east_of_utc_clock_offset() {
            assert_eq!(std::mem::size_of::<TimeZoneInformation>(), 172);
            assert_eq!(east_of_utc(300, 0).expect("standard"), -300);
            assert_eq!(east_of_utc(300, -60).expect("daylight"), -240);
            assert_eq!(east_of_utc(-60, 0).expect("ahead"), 60);
        }
    }
}

#[cfg(windows)]
pub use windows::local_offset_minutes;

#[cfg(not(windows))]
pub fn local_offset_minutes() -> std::io::Result<i32> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "local scheduling needs Windows clock information",
    ))
}
