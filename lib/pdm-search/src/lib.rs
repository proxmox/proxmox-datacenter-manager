//! Abstraction over a [`Search`] that contains multiple [`SearchTerm`]s.
//!
//! Provides methods to filter an item over a combination of such terms and
//! construct them from text, and serialize them back to text.
use std::fmt;

#[derive(Default, Clone)]
pub struct Search {
    required_terms: Vec<SearchTerm>,
    optional_terms: Vec<SearchTerm>,
}

impl FromIterator<SearchTerm> for Search {
    fn from_iter<T: IntoIterator<Item = SearchTerm>>(iter: T) -> Self {
        let (optional_terms, required_terms) = iter.into_iter().partition(|term| term.optional);

        Self {
            required_terms,
            optional_terms,
        }
    }
}

impl<S: AsRef<str>> From<S> for Search {
    fn from(value: S) -> Self {
        value
            .as_ref()
            .split_whitespace()
            .map(SearchTerm::from)
            .collect()
    }
}

impl Search {
    /// Create a new empty [`Search`]
    pub fn new() -> Self {
        Self::with_terms(Vec::new())
    }

    /// Returns true if no [`SearchTerm`] exist
    pub fn is_empty(&self) -> bool {
        self.required_terms.is_empty() && self.optional_terms.is_empty()
    }

    /// Create a new [`Search`] with the given [`SearchTerm`]s
    pub fn with_terms<I: IntoIterator<Item = SearchTerm>>(terms: I) -> Self {
        terms.into_iter().collect()
    }

    /// Test if the given `Fn(&SearchTerm) -> bool` for all [`SearchTerm`] configured matches
    ///
    /// Returns true if it matches considering the constraints:
    /// if there are no filters, returns true
    pub fn matches<F: FnMut(&SearchTerm) -> bool>(&self, mut matches: F) -> bool {
        if self.is_empty() {
            return true;
        }

        if self.required_terms.iter().map(&mut matches).any(|f| !f) {
            return false;
        }

        if !self.optional_terms.is_empty()
            && self.optional_terms.iter().map(&mut matches).all(|f| !f)
        {
            return false;
        }

        true
    }

    /// Add a term to the search
    pub fn add_term(&mut self, term: SearchTerm) {
        if term.is_optional() {
            self.optional_terms.push(term);
        } else {
            self.required_terms.push(term);
        }
    }
}

impl fmt::Display for Search {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut sep = "";
        for term in self.required_terms.iter().chain(self.optional_terms.iter()) {
            write!(f, "{sep}{term}")?;
            sep = " ";
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchTerm {
    optional: bool,
    pub value: String,
    pub category: Option<String>,
}

impl SearchTerm {
    /// Creates a new [`SearchTerm`].
    pub fn new<S: Into<String>>(term: S) -> Self {
        Self {
            value: term.into(),
            optional: false,
            category: None,
        }
    }

    /// Builder style method to set the category
    pub fn category<S: ToString>(mut self, category: Option<S>) -> Self {
        self.category = category.map(|s| s.to_string());
        self
    }

    /// Builder style method to mark this [`SearchTerm`] as optional
    pub fn optional(mut self, optional: bool) -> Self {
        self.optional = optional;
        self
    }

    /// Returns if the search term is optional
    pub fn is_optional(&self) -> bool {
        self.optional
    }
}

impl<S: AsRef<str>> From<S> for SearchTerm {
    fn from(value: S) -> Self {
        let term = value.as_ref();
        let mut optional = true;
        let term = if let Some(rest) = term.strip_prefix("+") {
            if rest.is_empty() {
                term
            } else {
                optional = false;
                rest
            }
        } else {
            term
        };

        let (term, category) = match term.split_once(':') {
            Some((category, new_term)) if category.is_empty() || new_term.is_empty() => {
                (term, None)
            }
            Some((category, new_term)) => (new_term, Some(category)),
            None => (term, None),
        };

        SearchTerm::new(term).optional(optional).category(category)
    }
}

impl fmt::Display for SearchTerm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.optional {
            f.write_str("+")?;
        }

        if let Some(cat) = &self.category {
            f.write_str(cat)?;
            f.write_str(":")?;
        }

        f.write_str(&self.value)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Search, SearchTerm};

    #[test]
    fn parse_test_simple_filter() {
        assert_eq!(
            SearchTerm::from("foo"),
            SearchTerm::new("foo").optional(true),
        );
    }

    #[test]
    fn parse_test_requires_filter() {
        assert_eq!(SearchTerm::from("+foo"), SearchTerm::new("foo"),);
    }

    #[test]
    fn parse_test_category_filter() {
        assert_eq!(
            SearchTerm::from("foo:bar"),
            SearchTerm::new("bar").optional(true).category(Some("foo"))
        );
        assert_eq!(
            SearchTerm::from("+foo:bar"),
            SearchTerm::new("bar").category(Some("foo"))
        );
    }

    #[test]
    fn parse_test_special_filter() {
        assert_eq!(
            SearchTerm::from(":bar"),
            SearchTerm::new(":bar").optional(true)
        );
        assert_eq!(SearchTerm::from("+cat:"), SearchTerm::new("cat:"));
        assert_eq!(SearchTerm::from("+"), SearchTerm::new("+").optional(true));
        assert_eq!(SearchTerm::from(":"), SearchTerm::new(":").optional(true));
    }

    #[test]
    fn match_tests() {
        let search = Search::from_iter(vec![
            SearchTerm::new("required1").optional(false),
            SearchTerm::new("required2").optional(false),
            SearchTerm::new("optional1").optional(true),
            SearchTerm::new("optional2").optional(true),
        ]);

        // each case contains results for
        // required1, required2, optional1, optional2
        // and if it should match or not
        let cases = [
            ((true, true, true, false), true),
            ((true, true, false, true), true),
            ((true, true, true, true), true),
            ((true, true, false, false), false),
            ((true, false, false, false), false),
            ((false, false, false, false), false),
            ((false, true, false, false), false),
            ((false, false, true, true), false),
            ((false, true, true, true), false),
            ((true, false, true, true), false),
            ((true, false, true, false), false),
            ((false, true, true, false), false),
            ((false, false, true, false), false),
            ((true, false, false, true), false),
            ((false, true, false, true), false),
            ((false, false, false, true), false),
        ];
        for (input, expected) in cases {
            assert!(
                search.matches(|term| {
                    match term.value.as_str() {
                        "required1" => input.0,
                        "required2" => input.1,
                        "optional1" => input.2,
                        "optional2" => input.3,
                        _ => unreachable!(),
                    }
                }) == expected
            )
        }
    }

    #[test]
    fn test_display() {
        let term = SearchTerm::new("foo");
        assert_eq!("+foo", &term.to_string());

        let term = SearchTerm::new("foo").optional(true);
        assert_eq!("foo", &term.to_string());

        let term = SearchTerm::new("foo").optional(false);
        assert_eq!("+foo", &term.to_string());

        let term = SearchTerm::new("foo").category(Some("bar"));
        assert_eq!("+bar:foo", &term.to_string());

        let term = SearchTerm::new("foo").optional(true).category(Some("bar"));
        assert_eq!("bar:foo", &term.to_string());

        let term = SearchTerm::new("foo").optional(false).category(Some("bar"));
        assert_eq!("+bar:foo", &term.to_string());
    }
}
