//! Dealing with `-list`, `-alist` types in pve schemas.

pub mod list {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use proxmox_schema::Schema;

    pub fn serialize<S, T>(
        data: &[T],
        serializer: S,
        array_schema: &'static Schema,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        use serde::ser::{Error, SerializeSeq};

        let mut ser =
            proxmox_schema::ser::PropertyStringSerializer::new(String::new(), array_schema)
                .serialize_seq(Some(data.len()))
                .map_err(|err| S::Error::custom(err))?;

        for element in data {
            ser.serialize_element(element)
                .map_err(|err| S::Error::custom(err))?;
        }

        let out = ser.end().map_err(|err| S::Error::custom(err))?;
        serializer.serialize_str(&out)
    }

    pub fn deserialize<'de, D, T>(
        deserializer: D,
        array_schema: &'static Schema,
    ) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        use serde::de::Error;

        let string = std::borrow::Cow::<'de, str>::deserialize(deserializer)?;

        T::deserialize(proxmox_schema::de::SchemaDeserializer::new(
            string,
            array_schema,
        ))
        .map_err(|err| D::Error::custom(err))
    }
}
