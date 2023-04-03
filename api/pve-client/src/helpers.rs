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
