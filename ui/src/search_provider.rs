use yew::Callback;

use pwt::state::{SharedState, SharedStateObserver};

use pdm_search::Search;

#[derive(Clone, PartialEq)]
pub struct SearchProvider {
    state: SharedState<String>,
}

impl SearchProvider {
    pub fn new() -> Self {
        Self {
            state: SharedState::new("".into()),
        }
    }

    pub fn add_listener(
        &self,
        cb: impl Into<Callback<SharedState<String>>>,
    ) -> SharedStateObserver<String> {
        self.state.add_listener(cb)
    }

    pub fn search(&self, search_term: Search) {
        **self.state.write() = search_term.to_string();
    }
}

pub fn get_search_provider<T: yew::Component>(ctx: &yew::Context<T>) -> Option<SearchProvider> {
    let (provider, _context_listener) = ctx.link().context(Callback::from(|_| {}))?;

    Some(provider)
}
