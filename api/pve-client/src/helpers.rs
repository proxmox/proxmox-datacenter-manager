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
        // FIXME: percent-encode:
        query.extend(percent_encoding::percent_encode(
            value.to_string().as_bytes(),
            percent_encoding::NON_ALPHANUMERIC,
        ));
        query.push_str(&value.to_string());
    }
}
