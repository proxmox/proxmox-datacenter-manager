/// Add an optional string parameter to the query, and if it was added, change `separator` to `&`.
pub fn add_query_arg<T>(query: &mut String, separator: &mut char, name: &str, value: &Option<T>)
where
    T: std::fmt::Display,
{
    if let Some(value) = value {
        query.push(*separator);
        *separator = '&';
        query.push_str(name);
        query.push('=');
        query.extend(percent_encoding::percent_encode(
            value.to_string().as_bytes(),
            percent_encoding::NON_ALPHANUMERIC,
        ));
    }
}

/// Add an optional boolean parameter to the query, and if it was added, change `separator` to `&`.
pub fn add_query_bool(query: &mut String, separator: &mut char, name: &str, value: Option<bool>) {
    if let Some(value) = value {
        query.push(*separator);
        *separator = '&';
        query.push_str(name);
        query.push_str(if value { "=1" } else { "=0" });
    }
}

pub trait PveQueryArg {
    fn pve_query_arg(&self, q: &mut String);
}

macro_rules! generate_array_field {
    ($type_name:ident :
     $(#[$doc:meta])*
     $field_type:ty => $api_def:tt
     $($field_names:ident),+ $(,)?) => {
        #[api(
            properties: {
                $( $field_names: $api_def, )*
            },
        )]
        $(#[$doc])*
        #[derive(Debug, serde::Deserialize, serde::Serialize)]
        pub struct $type_name {
            $(
                #[serde(skip_serializing_if = "Option::is_none")]
                $field_names: Option<$field_type>,
            )+
        }
    };
}

#[rustfmt::skip]
macro_rules! generate_string_list_type {
    ($array_type:ident for $content_type:ty => $array_schema_name:ident, $list_of_desc:literal) => {
        const $array_schema_name: Schema =
            proxmox_schema::ArraySchema::new($list_of_desc, &<$content_type>::API_SCHEMA).schema();

        impl proxmox_schema::ApiType for $array_type {
            const API_SCHEMA: Schema = proxmox_schema::StringSchema::new($list_of_desc)
                .format(&proxmox_schema::ApiStringFormat::PropertyString(&$array_schema_name))
                .schema();
        }

        #[derive(Debug)]
        pub struct $array_type(pub Vec<$content_type>);

        impl serde::Serialize for $array_type {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                crate::stringlist::list::serialize(&self.0, serializer, &$array_schema_name)
            }
        }

        impl<'de> serde::de::Deserialize<'de> for $array_type {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                Ok(Self(crate::stringlist::list::deserialize(
                    deserializer,
                    &$array_schema_name,
                )?))
            }
        }
    };
}
