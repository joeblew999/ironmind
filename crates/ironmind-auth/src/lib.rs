use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
};
use ironmind_r2::{store::ConversationStore, UserProfile};
use std::sync::Arc;
use tracing::warn;

pub struct AuthUser(pub UserProfile);

pub trait HasStore {
    fn store(&self) -> &ConversationStore;
}

impl<S> FromRequestParts<Arc<S>> for AuthUser
where
    S: HasStore + Send + Sync + 'static,
{
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<S>,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or(AuthError::Missing)?;

        let token_hash = blake3::hash(token.as_bytes()).to_hex().to_string();
        let user_id = token.split(':').next().ok_or(AuthError::Invalid)?;

        let user = state
            .store()
            .get_user(user_id)
            .await
            .map_err(|_| AuthError::Internal)?
            .ok_or(AuthError::Invalid)?;

        if user.token_hash != token_hash {
            warn!(user_id, "Token mismatch");
            return Err(AuthError::Invalid);
        }

        Ok(AuthUser(user))
    }
}

#[derive(Debug)]
pub enum AuthError {
    Missing,
    Invalid,
    Internal,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AuthError::Missing => (StatusCode::UNAUTHORIZED, "Authorization header required"),
            AuthError::Invalid => (StatusCode::UNAUTHORIZED, "Invalid token"),
            AuthError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "Auth error"),
        };
        (status, msg).into_response()
    }
}

pub fn generate_token(user_id: &str) -> (String, String) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    let secret = blake3::hash(format!("{}{}", user_id, nanos).as_bytes())
        .to_hex()
        .to_string();
    let token = format!("{}:{}", user_id, &secret[..32]);
    let hash = blake3::hash(token.as_bytes()).to_hex().to_string();
    (token, hash)
}
