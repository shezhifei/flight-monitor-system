use super::helpers::extract_flight_number;

#[test]
fn extract_flight_number_supports_existing_compact_format() {
    assert_eq!(
        extract_flight_number("查看 MU2451 的当前保障信息"),
        Some("MU2451".to_string())
    );
    assert_eq!(extract_flight_number("status for ca123"), Some("CA123".to_string()));
    assert_eq!(extract_flight_number("航班号是 ab12345"), Some("AB12345".to_string()));
}

#[test]
fn extract_flight_number_ignores_existing_supported_separators() {
    assert_eq!(extract_flight_number("查看 mu-2451"), Some("MU2451".to_string()));
    assert_eq!(extract_flight_number("查看 M U 2451"), Some("MU2451".to_string()));
    assert_eq!(extract_flight_number("查看 c_z_3_1_6"), Some("CZ316".to_string()));
}

#[test]
fn extract_flight_number_preserves_original_digit_bounds() {
    assert_eq!(extract_flight_number("MU12"), None);
    assert_eq!(extract_flight_number("MU123456"), Some("MU12345".to_string()));
}
