// OAuth 2.0 authentication with PKCE and CSRF protection

use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use tokio::sync::Mutex;

use oauth2::{
    basic::BasicClient, AuthUrl, ClientId, CsrfToken, PkceCodeChallenge,
    RedirectUrl, TokenUrl,
};

/// OAuth authentication state
#[derive(Clone)]
pub struct OAuthState {
    pub csrf_token: CsrfToken,
    pub pkce_verifier: Arc<Mutex<Option<String>>>,
    pub pkce_challenge: PkceCodeChallenge,
    pub client: Arc<BasicClient>,
    pub socket_addr: SocketAddr,
    pub auth_code: Arc<Mutex<Option<String>>>,
}

impl OAuthState {
    pub fn new(
        client_id: String,
        auth_url: String,
        token_url: String,
    ) -> Result<Self, String> {
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let socket_addr = get_available_addr().map_err(|e| format!("Failed to get socket: {}", e))?;

        // Build redirect URL with the actual socket address
        let redirect_url = format!("http://{}/callback", socket_addr);
        let redirect_url = RedirectUrl::new(redirect_url)
            .map_err(|e| format!("Invalid redirect URL: {}", e))?;

        let client = create_oauth_client(client_id, auth_url, token_url, redirect_url)?;

        Ok(Self {
            csrf_token: CsrfToken::new_random(),
            pkce_verifier: Arc::new(Mutex::new(Some(pkce_verifier.secret().to_string()))),
            pkce_challenge,
            client: Arc::new(client),
            socket_addr,
            auth_code: Arc::new(Mutex::new(None)),
        })
    }
}

/// Get an available socket address on localhost
fn get_available_addr() -> Result<SocketAddr, std::io::Error> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    drop(listener);
    Ok(addr)
}

/// Create OAuth2 client
fn create_oauth_client(
    client_id: String,
    auth_url: String,
    token_url: String,
    redirect_url: RedirectUrl,
) -> Result<BasicClient, String> {
    let client_id = ClientId::new(client_id);

    let auth_url = AuthUrl::new(auth_url)
        .map_err(|e| format!("Invalid auth URL: {}", e))?;

    let token_url = TokenUrl::new(token_url)
        .map_err(|e| format!("Invalid token URL: {}", e))?;

    Ok(BasicClient::new(
        client_id,
        None, // No client secret for native apps
        auth_url,
        Some(token_url),
    )
    .set_redirect_uri(redirect_url))
}
