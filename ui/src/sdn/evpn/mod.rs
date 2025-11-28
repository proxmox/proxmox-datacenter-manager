mod evpn_panel;
pub use evpn_panel::EvpnPanel;

mod remote_tree;
pub use remote_tree::RemoteTree;

mod vrf_tree;
pub use vrf_tree::VrfTree;

mod add_vnet;
pub use add_vnet::AddVnetWindow;

mod add_zone;
pub use add_zone::AddZoneWindow;

mod zone_status;
pub use zone_status::ZoneStatusTable;

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

#[derive(Debug, Clone, PartialEq, Default)]
#[repr(transparent)]
pub struct NodeList(Vec<String>);

impl std::ops::Deref for NodeList {
    type Target = Vec<String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::str::FromStr for NodeList {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            anyhow::bail!("node list cannot be an empty string");
        }

        Ok(Self(value.split(",").map(String::from).collect()))
    }
}

impl FromIterator<String> for NodeList {
    fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}
