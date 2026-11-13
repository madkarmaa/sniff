use super::*;

#[test]
fn ipv4_clients_are_counted_per_address() {
    assert_eq!(client_key("203.0.113.7").as_deref(), Some("203.0.113.7"));
    assert_eq!(client_key(" 203.0.113.7 ").as_deref(), Some("203.0.113.7"));
}

#[test]
fn ipv6_clients_are_counted_per_64_network() {
    let first = client_key("2001:db8:aa:bb:1:2:3:4");
    let second = client_key("2001:0db8:00aa:00bb:ffff::9");
    assert_eq!(first.as_deref(), Some("2001:db8:aa:bb::/64"));
    assert_eq!(first, second);
    assert_ne!(first, client_key("2001:db8:aa:bc::1"));
}

#[test]
fn ipv4_mapped_addresses_count_as_ipv4() {
    assert_eq!(
        client_key("::ffff:198.51.100.2").as_deref(),
        Some("198.51.100.2")
    );
}

#[test]
fn unparsable_addresses_have_no_key() {
    assert_eq!(client_key(""), None);
    assert_eq!(client_key("not-an-ip"), None);
}
