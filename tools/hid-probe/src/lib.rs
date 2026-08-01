pub mod descriptor;
pub mod hardware;
pub mod trace;

pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn parse_hex_u16(value: &str) -> Result<u16, String> {
    let value = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);

    if value.is_empty() || value.len() > 4 {
        return Err("expected a 16-bit hexadecimal value (for example 05ac)".to_owned());
    }

    u16::from_str_radix(value, 16).map_err(|_| format!("invalid 16-bit hexadecimal value: {value}"))
}

#[cfg(test)]
mod tests {
    use super::parse_hex_u16;

    #[test]
    fn parses_prefixed_and_unprefixed_hex() {
        assert_eq!(parse_hex_u16("05ac"), Ok(0x05ac));
        assert_eq!(parse_hex_u16("0X0267"), Ok(0x0267));
    }

    #[test]
    fn rejects_out_of_range_values() {
        assert!(parse_hex_u16("10000").is_err());
        assert!(parse_hex_u16("").is_err());
    }
}
