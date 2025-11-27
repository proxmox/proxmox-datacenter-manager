/// Helper wrapper to factor out some common api loading behavior
pub struct LoadResult<T, E> {
    pub data: Option<T>,
    pub error: Option<E>,
}

impl<T, E> LoadResult<T, E> {
    /// Creates a new empty result that contains no data or error.
    pub fn new() -> Self {
        Self {
            data: None,
            error: None,
        }
    }

    /// Update the current value with the given result
    ///
    /// On `Ok`, the previous error will be deleted.
    /// On `Err`, the previous valid date is kept.
    pub fn update(&mut self, result: Result<T, E>) {
        match result {
            Ok(data) => {
                self.error = None;
                self.data = Some(data);
            }
            Err(err) => {
                self.error = Some(err);
            }
        }
    }

    /// If any of data or err has any value
    pub fn has_data(&self) -> bool {
        self.data.is_some() || self.error.is_some()
    }

    /// Clears both data and the error from the result.
    pub fn clear(&mut self) {
        self.data = None;
        self.error = None;
    }
}

impl<T, E> Default for LoadResult<T, E> {
    fn default() -> Self {
        Self::new()
    }
}
