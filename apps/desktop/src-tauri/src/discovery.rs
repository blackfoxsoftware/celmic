//! LAN addresses and DNS-SD advertisement (`_celmic._udp`).

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};

use mdns_sd::{ServiceDaemon, ServiceInfo};

pub const SERVICE_TYPE: &str = "_celmic._udp.local.";

/// Usable IPv4 addresses of this machine, most likely LAN address first.
///
/// Virtual adapters (WSL, Docker, Hyper-V, VPNs) are kept but pushed to the end;
/// the phone races all candidates, so ordering only affects the QR/TXT size.
pub fn lan_ipv4s() -> Vec<Ipv4Addr> {
    let mut ips: Vec<(i32, Ipv4Addr)> = local_ip_address::list_afinet_netifas()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(iface, ip)| match ip {
            IpAddr::V4(v4) if !v4.is_loopback() && !v4.is_link_local() && !v4.is_unspecified() => {
                Some((score(&iface, v4), v4))
            }
            _ => None,
        })
        .collect();
    ips.sort_by_key(|(score, _)| -score);
    ips.dedup_by_key(|(_, ip)| *ip);
    ips.into_iter().map(|(_, ip)| ip).collect()
}

fn score(iface: &str, ip: Ipv4Addr) -> i32 {
    let iface = iface.to_lowercase();
    let virtual_iface = ["vethernet", "wsl", "docker", "br-", "veth", "virbr", "vmnet", "vbox", "tailscale", "zerotier", "utun", "tun", "tap"]
        .iter()
        .any(|v| iface.contains(v));
    let [a, b, ..] = ip.octets();
    let range = match (a, b) {
        (192, 168) => 30,
        (10, _) => 20,
        (172, 16..=31) => 10,
        _ if ip.is_private() => 5,
        _ => 0,
    };
    range - if virtual_iface { 100 } else { 0 }
}

/// mDNS host label derived from the machine name (ASCII letters/digits/dashes).
fn host_label(name: &str) -> String {
    let mut label = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            label.push(c.to_ascii_lowercase());
        } else if !label.ends_with('-') {
            label.push('-');
        }
    }
    let label = label.trim_matches('-');
    if label.is_empty() { "celmic-desktop".into() } else { label.into() }
}

/// DNS-SD advertisement of this desktop. Dropping it stops advertising.
pub struct Advertiser {
    daemon: ServiceDaemon,
    name: String,
    port: u16,
}

impl Advertiser {
    pub fn start(name: &str, port: u16, ips: &[Ipv4Addr]) -> Result<Self, String> {
        let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
        let advertiser = Self { daemon, name: name.into(), port };
        advertiser.announce(ips)?;
        Ok(advertiser)
    }

    /// (Re-)announces the service; call again when the LAN addresses change.
    pub fn announce(&self, ips: &[Ipv4Addr]) -> Result<(), String> {
        let host_name = format!("{}.local.", host_label(&self.name));
        let properties = HashMap::from([
            ("v".to_string(), crate::protocol::VERSION.to_string()),
            ("name".to_string(), self.name.clone()),
            ("port".to_string(), self.port.to_string()),
            (
                "ips".to_string(),
                ips.iter().map(ToString::to_string).collect::<Vec<_>>().join(","),
            ),
        ]);

        let info = ServiceInfo::new(SERVICE_TYPE, &self.name, &host_name, "", self.port, properties)
            .map_err(|e| e.to_string())?
            .enable_addr_auto();
        self.daemon.register(info).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_home_lan_over_virtual_adapters() {
        let wsl = score("vEthernet (WSL)", Ipv4Addr::new(172, 20, 0, 1));
        let wifi = score("Wi-Fi", Ipv4Addr::new(192, 168, 0, 10));
        let corp = score("Ethernet", Ipv4Addr::new(10, 1, 2, 3));
        assert!(wifi > corp && corp > wsl);
    }

    #[test]
    fn host_label_is_always_valid() {
        assert_eq!(host_label("Rafael's PC"), "rafael-s-pc");
        assert_eq!(host_label("Ação"), "a-o");
        assert_eq!(host_label("日本語"), "celmic-desktop");
    }
}
