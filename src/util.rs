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
    use super::mask_vin;

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
}