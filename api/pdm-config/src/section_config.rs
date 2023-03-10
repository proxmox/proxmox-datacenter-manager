//! Experimental way to connect a `SectionConfig` to a proper rust datatype.
//!
//! To be eventually moved to `proxmox-section-config` with a derive macro for enums with only
//! newtype variants.

use std::collections::HashMap;

use anyhow::Error;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

use proxmox_section_config::SectionConfig;
use proxmox_section_config::SectionConfigData as RawSectionConfigData;

pub trait ApiSectionDataEntry: Sized {
    /// Get the `SectionConfig` configuration for this enum.
    fn section_config() -> &'static SectionConfig;

    /// Maps an enum value to its type name.
    fn section_type(&self) -> &'static str;

    fn from_value(ty: String, value: Value) -> Result<Self, serde_json::Error>
    where
        Self: serde::de::DeserializeOwned,
    {
        serde_json::from_value::<Self>(json!({ ty: value }))
    }

    /// The default implementation only succeeds for externally tagged enums (serde's default enum
    /// representation).
    fn to_pair_owned(self) -> Result<(String, Value), serde_json::Error>
    where
        Self: Serialize,
    {
        use serde::ser::Error;

        match serde_json::to_value(self)? {
            Value::Object(obj) if obj.len() == 1 => Ok(
                obj.into_iter().next().unwrap(), // unwrap: we checked the length
            ),
            _ => Err(Error::custom("unexpected serialization method")),
        }
    }

    /// The default implementation only succeeds for externally tagged enums (serde's default enum
    /// representation).
    fn to_pair(&self) -> Result<(String, Value), serde_json::Error>
    where
        Self: Serialize,
    {
        use serde::ser::Error;

        match serde_json::to_value(self)? {
            Value::Object(obj) if obj.len() == 1 => Ok(
                obj.into_iter().next().unwrap(), // unwrap: we checked the length
            ),
            _ => Err(Error::custom("unexpected serialization method")),
        }
    }

    fn parse_section_config(filename: &str, data: &str) -> Result<SectionConfigData<Self>, Error>
    where
        Self: serde::de::DeserializeOwned,
    {
        Ok(Self::section_config().parse(filename, data)?.try_into()?)
    }

    fn store_section_config(filename: &str, data: SectionConfigData<Self>) -> Result<String, Error>
    where
        Self: Serialize,
    {
        Self::section_config().write(filename, &data.try_into()?)
    }

    fn write_section_config(filename: &str, data: &SectionConfigData<Self>) -> Result<String, Error>
    where
        Self: Serialize,
    {
        Self::section_config().write(filename, &data.try_into()?)
    }
}

/// Types section config data.
#[derive(Debug, Clone)]
pub struct SectionConfigData<T> {
    pub sections: HashMap<String, T>,
    pub order: Vec<String>,
}

impl<T> Default for SectionConfigData<T> {
    fn default() -> Self {
        Self {
            sections: HashMap::new(),
            order: Vec::new(),
        }
    }
}

impl<T: ApiSectionDataEntry + DeserializeOwned> TryFrom<RawSectionConfigData>
    for SectionConfigData<T>
{
    type Error = serde_json::Error;

    fn try_from(data: RawSectionConfigData) -> Result<Self, serde_json::Error> {
        let sections =
            data.sections
                .into_iter()
                .try_fold(HashMap::new(), |mut acc, (id, (ty, value))| {
                    acc.insert(id, T::from_value(ty, value)?);
                    Ok::<_, serde_json::Error>(acc)
                })?;
        Ok(Self {
            sections,
            order: data.order,
        })
    }
}

impl<T> std::ops::Deref for SectionConfigData<T> {
    type Target = HashMap<String, T>;

    fn deref(&self) -> &Self::Target {
        &self.sections
    }
}

impl<T> std::ops::DerefMut for SectionConfigData<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.sections
    }
}

impl<T: Serialize + ApiSectionDataEntry> TryFrom<SectionConfigData<T>> for RawSectionConfigData {
    type Error = serde_json::Error;

    fn try_from(data: SectionConfigData<T>) -> Result<Self, serde_json::Error> {
        let sections =
            data.sections
                .into_iter()
                .try_fold(HashMap::new(), |mut acc, (id, value)| {
                    acc.insert(id, value.to_pair_owned()?);
                    Ok::<_, serde_json::Error>(acc)
                })?;

        Ok(Self {
            sections,
            order: data.order,
        })
    }
}

impl<T: Serialize + ApiSectionDataEntry> TryFrom<&SectionConfigData<T>> for RawSectionConfigData {
    type Error = serde_json::Error;

    fn try_from(data: &SectionConfigData<T>) -> Result<Self, serde_json::Error> {
        let sections = data
            .sections
            .iter()
            .try_fold(HashMap::new(), |mut acc, (id, value)| {
                acc.insert(id.clone(), value.to_pair()?);
                Ok::<_, serde_json::Error>(acc)
            })?;

        Ok(Self {
            sections,
            order: data.order.clone(),
        })
    }
}

/// Creates an unordered data set.
impl<T: ApiSectionDataEntry> From<HashMap<String, T>> for SectionConfigData<T> {
    fn from(sections: HashMap<String, T>) -> Self {
        Self {
            sections,
            order: Vec::new(),
        }
    }
}

/// Creates a data set ordered the same way as the iterator.
impl<T: ApiSectionDataEntry> FromIterator<(String, T)> for SectionConfigData<T> {
    fn from_iter<I: IntoIterator<Item = (String, T)>>(iter: I) -> Self {
        let mut sections = HashMap::new();
        let mut order = Vec::new();

        for (key, value) in iter {
            order.push(key.clone());
            sections.insert(key, value);
        }

        Self { sections, order }
    }
}
