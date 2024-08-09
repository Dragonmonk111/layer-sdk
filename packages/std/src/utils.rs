// TODO: move this to standard utils (also in storage/src/traits.rs)
pub fn stringify_or_hex(input: &[u8]) -> String {
    std::str::from_utf8(input).map_or_else(|_| hex::encode_upper(input), |x| x.to_string())
}
