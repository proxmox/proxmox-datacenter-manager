use pwt::state::{SharedState, SharedStateObserver};
use yew::Callback;

#[derive(PartialEq, Clone)]
/// Provides a context for updating and listening to changes of the list of views
pub struct ViewListContext {
    state: SharedState<usize>,
}

impl ViewListContext {
    /// Create a new context
    pub fn new() -> Self {
        Self {
            state: SharedState::new(0),
        }
    }

    /// Add a listener to the view list context
    pub fn add_listener(
        &self,
        cb: impl Into<Callback<SharedState<usize>>>,
    ) -> SharedStateObserver<usize> {
        self.state.add_listener(cb)
    }

    /// Triggers an update of the view list for the main menu
    pub fn update_views(&self) {
        let mut state = self.state.write();
        **state = state.saturating_add(1);
    }
}
