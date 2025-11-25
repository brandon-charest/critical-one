use crate::game::types::{GameError, GameId};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Game with ID {0} not found")]
    GameNotFound(GameId),

    #[error("Game logic violation: {0}")]
    Game(#[from] GameError),

    #[error("Access denied: {0}")]
    Forbidden(String),

    #[error("Internal server error: {0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::Redis(e) => {
                tracing::error!("Redis error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An internal database error occurred".to_string(),
                )
            }
            AppError::Serde(e) => {
                tracing::error!("Serialization error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An internal serialization error occurred".to_string(),
                )
            }
            AppError::GameNotFound(id) => (
                StatusCode::NOT_FOUND,
                format!("Game with id {} not found", id),
            ),
            AppError::Game(e) => {
                tracing::warn!("Game logic violation: {}", e);
                (
                    StatusCode::BAD_REQUEST,
                    format!("Game rule violation: {}", e),
                )
            }
            AppError::Forbidden(msg) => {
                tracing::warn!("Access denied: {}", msg);
                (StatusCode::FORBIDDEN, msg)
            }
            AppError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An unexpected error occurred".to_string(),
                )
            }
        };

        let body = Json(json!({ "error": error_message }));
        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::types::GameIdTestExt;
    use axum::body::{to_bytes, Body};
    use uuid::Uuid;

    async fn check_response(response: Response<Body>) -> (StatusCode, String) {
        let status = response.status();
        let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let error_message = body_json
            .get("error")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        (status, error_message)
    }

    #[tokio::test]
    async fn test_game_not_found_response() {
        let test_uuid = Uuid::parse_str("d3b07384-d937-4e3b-9e02-2b2c4d6e1232").unwrap();
        let test_game_id = GameIdTestExt::from_uuid(test_uuid);
        let error = AppError::GameNotFound(test_game_id);

        let response = error.into_response();
        let (status, message) = check_response(response).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(message.contains(&test_game_id.to_string()));
        assert!(message.contains("not found"));
    }

    #[tokio::test]
    async fn test_forbidden_response() {
        let error = AppError::Forbidden("GET OUT!".to_string());

        let response = error.into_response();
        let (status, message) = check_response(response).await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(message, "GET OUT!");
    }

    #[tokio::test]
    async fn test_game_rule_violation_response() {
        let error = AppError::Game(GameError::GameFull);

        let response = error.into_response();
        let (status, message) = check_response(response).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(message.contains("The game is full"));
    }
    #[tokio::test]
    async fn test_internal_server_error_response() {
        let error = AppError::Internal("Something blew up".to_string());
        let response = error.into_response();
        let (status, message) = check_response(response).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(message, "An unexpected error occurred"); // The generic message we return to client
    }
}
