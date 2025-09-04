mod remote_tree;
pub use remote_tree::RemoteTree;

mod vrf_tree;
pub use vrf_tree::VrfTree;

mod add_vnet;
pub use add_vnet::AddVnetWindow;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct EvpnRouteTarget {
    asn: u32,
    vni: u32,
}

impl std::str::FromStr for EvpnRouteTarget {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Some((asn, vni)) = value.split_once(':') {
            return Ok(Self {
                asn: asn.parse()?,
                vni: vni.parse()?,
            });
        }

        anyhow::bail!("could not parse EVPN route target!")
    }
}

impl std::fmt::Display for EvpnRouteTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "{}:{}", self.asn, self.vni)
    }
}
