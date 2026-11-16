use super::*;
use crate::web_board::security::{random_token, resource_path, token_matches};

fn interface(name: &str, address: &str, enabled: bool) -> InterfaceAddress {
    InterfaceAddress {
        name: name.into(),
        address: address.parse().unwrap(),
        enabled,
    }
}

#[test]
fn addresses_prefer_private_real_interfaces_and_filter_disabled_loopback_ipv6() {
    let addresses = sort_addresses([
        interface("Docker 0", "172.18.0.1", true),
        interface("en2", "169.254.4.5", true),
        interface("en1", "203.0.113.8", true),
        interface("en0", "192.168.1.8", true),
        interface("en3", "10.0.0.2", true),
        interface("vEthernet", "10.1.0.1", true),
        interface("VMware", "192.168.9.1", true),
        interface("utun4", "10.2.0.1", true),
        interface("Virtual NIC", "10.3.0.1", true),
        interface("en0", "192.168.1.8", true),
        interface("disabled", "192.168.1.1", false),
        interface("lo0", "127.0.0.1", true),
        interface("en0", "::1", true),
        interface("zero", "0.0.0.0", true),
        interface("multicast", "224.0.0.1", true),
        interface("broadcast", "255.255.255.255", true),
    ]);
    assert_eq!(
        addresses
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        [
            "10.0.0.2",
            "192.168.1.8",
            "203.0.113.8",
            "10.1.0.1",
            "10.2.0.1",
            "10.3.0.1",
            "172.18.0.1",
            "192.168.9.1",
            "169.254.4.5"
        ]
    );
}

#[test]
fn credentials_have_128_random_bits_and_canonical_paths_only() {
    let token = random_token().unwrap();
    let another = random_token().unwrap();
    assert_eq!(token.len(), 22);
    assert!(
        token
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
    );
    assert!(!token_matches(&token, &another));
    assert!(token_matches(&token, &token));
    assert!(!token_matches(&token, &token[..21]));
    assert!(!token_matches(&token, &format!("{token}x")));
    assert_eq!(
        resource_path(&format!("/{token}/"), &token),
        Some("web.html")
    );
    assert_eq!(resource_path(&format!("/{token}/ws"), &token), Some("ws"));
    assert_eq!(
        resource_path(&format!("/{token}/assets/main.js"), &token),
        Some("assets/main.js")
    );
    for path in [
        "assets/../secret",
        "assets/%2e%2e/secret",
        "assets/a%2fb",
        "assets/a%5Cb",
        "assets/\\secret",
        "assets//secret",
        "settings.json",
        "assets/",
    ] {
        assert!(
            resource_path(&format!("/{token}/{path}"), &token).is_none(),
            "{path}"
        );
    }
}
