use std::net::{IpAddr, Ipv4Addr};

#[derive(Debug, Clone)]
pub(super) struct InterfaceAddress {
    pub name: String,
    pub address: IpAddr,
    pub enabled: bool,
}

/// The platform boundary is deliberately limited to enumerating interface facts.
pub(crate) fn local_addresses() -> std::io::Result<Vec<Ipv4Addr>> {
    Ok(sort_addresses(if_addrs::get_if_addrs()?.into_iter().map(
        |interface| InterfaceAddress {
            address: interface.ip(),
            enabled: interface.is_oper_up(),
            name: interface.name,
        },
    )))
}

pub(super) fn sort_addresses(
    interfaces: impl IntoIterator<Item = InterfaceAddress>,
) -> Vec<Ipv4Addr> {
    let mut candidates: Vec<_> = interfaces
        .into_iter()
        .filter_map(|interface| {
            let IpAddr::V4(address) = interface.address else {
                return None;
            };
            if !interface.enabled
                || address.is_loopback()
                || address.is_unspecified()
                || address.is_multicast()
                || address.is_broadcast()
            {
                return None;
            }
            let name = interface.name.to_ascii_lowercase();
            let virtual_interface = ["virtual", "vmware", "vethernet", "docker", "utun"]
                .iter()
                .any(|marker| name.contains(marker));
            let low_priority = virtual_interface || address.is_link_local();
            Some((low_priority, !address.is_private(), address, name))
        })
        .collect();
    candidates.sort();
    let mut addresses = Vec::new();
    for (_, _, address, _) in candidates {
        if !addresses.contains(&address) {
            addresses.push(address);
        }
    }
    addresses
}

#[cfg(test)]
#[path = "address_tests.rs"]
mod tests;
