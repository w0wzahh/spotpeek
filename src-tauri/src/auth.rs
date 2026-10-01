use axum::{
    extract::Query,
    response::Html,
    routing::get,
    Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use oauth2::CsrfToken;
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::net::TcpListener;
use tokio::time::{timeout, Duration};

const SPOTIFY_AUTH_URL: &str = "https://accounts.spotify.com/authorize";
const SPOTIFY_TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
const REDIRECT_HOST: &str = "127.0.0.1";
const REDIRECT_PORT: u16 = 9133;

pub struct AuthFlow {
    pub auth_url: String,
    pub pkce_verifier: String,
    pub csrf_token: String,
}

pub fn generate_pkce() -> (String, String) {
    let mut verifier_bytes = [0u8; 64];
    rand::thread_rng().fill_bytes(&mut verifier_bytes);
    let verifier = URL_SAFE_NO_PAD.encode(&verifier_bytes);

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

    (verifier, challenge)
}

pub fn build_auth_url(client_id: &str, scopes: &[&str]) -> AuthFlow {
    let (verifier, challenge) = generate_pkce();
    let csrf = CsrfToken::new_random();
    let csrf_value = csrf.secret().to_string();

    let mut auth_url = url::Url::parse(SPOTIFY_AUTH_URL).unwrap();
    {
        let mut pairs = auth_url.query_pairs_mut();
        pairs.append_pair("client_id", client_id);
        pairs.append_pair("response_type", "code");
        pairs.append_pair("redirect_uri", &format!("http://{}:{}/callback", REDIRECT_HOST, REDIRECT_PORT));
        pairs.append_pair("code_challenge_method", "S256");
        pairs.append_pair("code_challenge", &challenge);
        pairs.append_pair("state", &csrf_value);
        let scope_str = scopes.join(" ");
        pairs.append_pair("scope", &scope_str);
    }

    AuthFlow {
        auth_url: auth_url.to_string(),
        pkce_verifier: verifier,
        csrf_token: csrf_value,
    }
}

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

#[derive(Debug, Clone)]
pub struct TokenResponse {
    pub access_token: String,
    pub expires_in: u64,
    pub refresh_token: Option<String>,
}

pub async fn start_callback_server(
    expected_state: String,
    client_id: String,
    pkce_verifier: String,
) -> Result<(TokenResponse, SocketAddr), String> {
    let (tx, mut rx) = mpsc::channel::<Result<TokenResponse, String>>(1);
    let tx = Arc::new(Mutex::new(tx));

    let listener = TcpListener::bind(format!("127.0.0.1:{}", REDIRECT_PORT))
        .await
        .map_err(|e| format!("Failed to bind callback server: {}", e))?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;

    let app = Router::new().route(
        "/callback",
        get({
            let tx = tx.clone();
            let expected_state = expected_state.clone();
            let client_id = client_id.clone();
            let pkce_verifier = pkce_verifier.clone();
            move |query: Query<CallbackQuery>| {
                let tx = tx.clone();
                let expected_state = expected_state.clone();
                let client_id = client_id.clone();
                let pkce_verifier = pkce_verifier.clone();
                async move {
                    let Query(params) = query;
                    if params.state != expected_state {
                        let _ = tx.lock().await.send(Err("CSRF state mismatch".to_string())).await;
                        return Html("<h3>Authentication failed: CSRF mismatch</h3>".to_string());
                    }

                    let res = crate::spotify::CLIENT
                        .post(SPOTIFY_TOKEN_URL)
                        .form(&[
                            ("grant_type", "authorization_code"),
                            ("code", &params.code),
                            (
                                "redirect_uri",
                                &format!("http://{}:{}/callback", REDIRECT_HOST, REDIRECT_PORT),
                            ),
                            ("client_id", &client_id),
                            ("code_verifier", &pkce_verifier),
                        ])
                        .send()
                        .await;

                    match res {
                        Ok(resp) => {
                            if resp.status().is_success() {
                                match resp.json::<serde_json::Value>().await {
                                    Ok(json) => {
                                        let token = TokenResponse {
                                            access_token: json["access_token"].as_str().unwrap_or("").to_string(),
                                            expires_in: json["expires_in"].as_u64().unwrap_or(3600),
                                            refresh_token: json["refresh_token"].as_str().map(|s| s.to_string()),
                                        };
                                        let _ = tx.lock().await.send(Ok(token)).await;
                                        Html("<h3>SpotPeek authenticated successfully! You can close this window.</h3>".to_string())
                                    }
                                    Err(e) => {
                                        let _ = tx.lock().await.send(Err(format!("JSON parse error: {}", e))).await;
                                        Html("<h3>Authentication failed: invalid response</h3>".to_string())
                                    }
                                }
                            } else {
                                let _ = tx.lock().await.send(Err(format!("Token exchange failed: {}", resp.status()))).await;
                                Html("<h3>Authentication failed: token exchange error</h3>".to_string())
                            }
                        }
                        Err(e) => {
                            let _ = tx.lock().await.send(Err(format!("Request error: {}", e))).await;
                            Html("<h3>Authentication failed: network error</h3>".to_string())
                        }
                    }
                }
            }
        }),
    );

    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let result = match timeout(Duration::from_secs(300), rx.recv()).await {
        Ok(Some(inner)) => inner,
        Ok(None) => Err("Callback channel closed".to_string()),
        Err(_) => Err("Authentication timed out. Please try again.".to_string()),
    };

    server.abort();
    result.map(|t| (t, addr))
}

pub async fn refresh_access_token(refresh_token: &str, client_id: &str) -> Result<TokenResponse, String> {
    let res = crate::spotify::CLIENT
        .post(SPOTIFY_TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &client_id),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(format!("Refresh failed: {} — {}", status, body));
    }

    let json = res.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
    Ok(TokenResponse {
        access_token: json["access_token"].as_str().unwrap_or("").to_string(),
        expires_in: json["expires_in"].as_u64().unwrap_or(3600),
        refresh_token: json["refresh_token"].as_str().map(|s| s.to_string()),
    })
}
