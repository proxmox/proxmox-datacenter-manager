use crate::Authentication;

/// How the client is logged in to the remote.
pub enum AuthenticationKind {
    /// With an API Ticket.
    Ticket(Authentication),

    /// With a token.
    Token(Token),
}

impl AuthenticationKind {
    pub fn set_auth_headers(&self, request: http::request::Builder) -> http::request::Builder {
        match self {
            AuthenticationKind::Ticket(auth) => auth.set_auth_headers(request),
            AuthenticationKind::Token(auth) => auth.set_auth_headers(request),
        }
    }

    pub fn userid(&self) -> &str {
        match self {
            AuthenticationKind::Ticket(auth) => &auth.userid,
            AuthenticationKind::Token(auth) => &auth.userid,
        }
    }
}

impl From<Authentication> for AuthenticationKind {
    fn from(auth: Authentication) -> Self {
        Self::Ticket(auth)
    }
}

impl From<Token> for AuthenticationKind {
    fn from(auth: Token) -> Self {
        Self::Token(auth)
    }
}

/// Data used to log in with a token.
pub struct Token {
    /// The userid.
    pub userid: String,

    /// The api token name (usually the product abbreviation).
    pub prefix: String,

    /// The api token's value.
    pub value: String,
}

impl Token {
    pub fn set_auth_headers(&self, request: http::request::Builder) -> http::request::Builder {
        request.header(
            http::header::AUTHORIZATION,
            format!("{}={}={}", self.prefix, self.userid, self.value),
        )
    }
}
