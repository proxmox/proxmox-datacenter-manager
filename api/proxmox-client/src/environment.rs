use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use http::Uri;

use proxmox_login::tfa::TfaChallenge;

use crate::Error;

/// Provide input from the environment for storing/loading tickets or tokens and querying the user
/// for passwords or 2nd factors.
pub trait Environment: Send + Sync {
    type Error: Error;

    /// Store a ticket belonging to a user of an API.
    ///
    /// This is only used if `store_ticket_async` is not overwritten and may be left unimplemented
    /// in async code. By default it will just return an error.
    ///
    /// [`store_ticket_async`]: Environment::store_ticket_async
    fn store_ticket(&self, api_url: &Uri, userid: &str, ticket: &[u8]) -> Result<(), Self::Error> {
        let _ = (api_url, userid, ticket);
        Err(Self::Error::custom(
            "missing store_ticket(_async) implementation",
        ))
    }

    /// Load a user's cached ticket for an API url.
    ///
    /// This is only used if [`load_ticket_async`] is not overwritten and may be left unimplemented
    /// in async code. By default it will just return an error.
    ///
    /// [`load_ticket_async`]: Environment::load_ticket_async
    fn load_ticket(&self, api_url: &Uri, userid: &str) -> Result<Option<Vec<u8>>, Self::Error> {
        let _ = (api_url, userid);
        Err(Self::Error::custom(
            "missing load_ticket(_async) implementation",
        ))
    }

    /// Query for a userid (name and realm).
    ///
    /// This is only used if [`query_userid_async`] is not overwritten and may be left
    /// unimplemented in async code. By default it will just return an error.
    ///
    /// [`query_userid_async`]: Environment::query_userid_async
    fn query_userid(&self, api_url: &Uri) -> Result<String, Self::Error> {
        let _ = api_url;
        Err(Self::Error::custom(
            "missing query_userid(_async) implementation",
        ))
    }

    /// Query for a password.
    ///
    /// This is only used if [`query_password_async`] is not overwritten and may be left
    /// unimplemented in async code. By default it will just return an error.
    ///
    /// [`query_password_async`]: Environment::query_password_async
    fn query_password(&self, api_url: &Uri, userid: &str) -> Result<String, Self::Error> {
        let _ = (api_url, userid);
        Err(Self::Error::custom(
            "missing query_password(_async) implementation",
        ))
    }

    /// Query for a second factor. The default implementation is to not support 2nd factors.
    ///
    /// This is only used if [`query_second_factor_async`] is not overwritten and may be left
    /// unimplemented in async code. By default it will just return an error.
    ///
    /// [`query_second_factor_async`]: Environment::query_second_factor_async
    fn query_second_factor(
        &self,
        api_url: &Uri,
        userid: &str,
        challenge: &TfaChallenge,
    ) -> Result<String, Self::Error> {
        let _ = (api_url, userid, challenge);
        Err(Self::Error::second_factor_not_supported())
    }

    /// The client code uses async rust and it is fine to implement this instead of `store_ticket`.
    fn store_ticket_async<'a>(
        &'a self,
        api_url: &'a Uri,
        userid: &'a str,
        ticket: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), Self::Error>> + Send + 'a>> {
        Box::pin(async move { self.store_ticket(api_url, userid, ticket) })
    }

    #[allow(clippy::type_complexity)]
    fn load_ticket_async<'a>(
        &'a self,
        api_url: &'a Uri,
        userid: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Vec<u8>>, Self::Error>> + Send + 'a>> {
        Box::pin(async move { self.load_ticket(api_url, userid) })
    }

    fn query_userid_async<'a>(
        &'a self,
        api_url: &'a Uri,
    ) -> Pin<Box<dyn Future<Output = Result<String, Self::Error>> + Send + 'a>> {
        Box::pin(async move { self.query_userid(api_url) })
    }

    fn query_password_async<'a>(
        &'a self,
        api_url: &'a Uri,
        userid: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String, Self::Error>> + Send + 'a>> {
        Box::pin(async move { self.query_password(api_url, userid) })
    }

    fn query_second_factor_async<'a>(
        &'a self,
        api_url: &'a Uri,
        userid: &'a str,
        challenge: &'a TfaChallenge,
    ) -> Pin<Box<dyn Future<Output = Result<String, Self::Error>> + Send + 'a>> {
        Box::pin(async move { self.query_second_factor(api_url, userid, challenge) })
    }

    /// In order to allow the polling based task API to function, we need a way to sleep in async
    /// context.
    /// This will likely be removed when the streaming tasks API is available.
    ///
    /// # Panics
    ///
    /// The default implementation simply panics.
    fn sleep(
        time: Duration,
    ) -> Result<Pin<Box<dyn Future<Output = ()> + Send + 'static>>, Self::Error> {
        let _ = time;
        Err(Self::Error::sleep_not_supported())
    }
}
