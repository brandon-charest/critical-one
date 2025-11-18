use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use crate::game::types::{GameId, GameError};
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
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::Redis(e) => {
                tracing::error!("Redis error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "An internal database error occurred".to_string())
            }
            AppError::Serde(e) => {
                tracing::error!("Serialization error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "An internal serialization error occurred".to_string())
            }
            AppError::GameNotFound(id) => {
                (StatusCode::NOT_FOUND, format!("Game with id {} not found", id))
            }
            AppError::Game(e) => {
                // Use the Display implementation of GameError for the message
                tracing::warn!("Game logic violation: {}", e);
                (StatusCode::BAD_REQUEST, format!("Game rule violation: {}", e))
            }
        };

        let body = Json(json!({ "error": error_message }));
        (status, body).into_response()
    }
}