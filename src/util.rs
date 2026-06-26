use chrono::{DateTime, Local, Utc};

pub fn format_local_timestamp(time: DateTime<Utc>) -> String {
    time.with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

pub fn mask_vin(vin: &str) -> String {
    let chars: Vec<char> = vin.chars().collect();
    match chars.len() {
        0 => String::new(),
        1 | 2 => vin.to_string(),
        len => {
            let mut masked = String::with_capacity(len);
            masked.push(chars[0]);
            masked.extend(std::iter::repeat_n('*', len - 2));
            masked.push(chars[len - 1]);
            masked
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::{format_local_timestamp, mask_vin};

    #[test]
    fn masks_standard_vin() {
        assert_eq!(mask_vin("5YJSA11111111111"), "5**************1");
    }

    #[test]
    fn leaves_short_vins_unmasked() {
        assert_eq!(mask_vin(""), "");
        assert_eq!(mask_vin("A"), "A");
        assert_eq!(mask_vin("AB"), "AB");
    }

    #[test]
    fn masks_three_character_vin() {
        assert_eq!(mask_vin("ABC"), "A*C");
    }

    #[test]
    fn format_local_timestamp_omits_utc_suffix() {
        let time = Utc.with_ymd_and_hms(2026, 6, 22, 18, 24, 10).unwrap();
        let formatted = format_local_timestamp(time);
        assert!(!formatted.contains("UTC"));
        assert!(formatted.starts_with("2026-06-22 "));
    }
}