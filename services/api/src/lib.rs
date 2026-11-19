//! Shirori API: a modular monolith (architecture §5.3).

pub mod auth;
pub mod catalogue;
pub mod config;
pub mod editorial;
pub mod error;
pub mod events;
pub mod feed;
pub mod saved;
pub mod seed;
pub mod state;
pub mod users;
mod web;

use axum::{
    Json, Router,
    routing::{get, post, put},
};
use serde_json::{Value, json};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

pub use state::AppState;

pub use shirori_persistence::MIGRATOR;

pub fn router(state: AppState) -> Router {
    let mut app = Router::new()
        .route("/health", get(health))
        .route("/v1/feed/daily", get(feed::get_daily))
        .route("/v1/feed/daily/{dose_id}/complete", post(feed::complete))
        .route("/v1/feed/discover", get(feed::discover))
        .route("/v1/cards/{id}", get(catalogue::get_card))
        .route("/v1/cards/{id}/sources", get(catalogue::get_sources))
        .route("/v1/cards/{id}/connections", get(catalogue::get_connections))
        .route("/v1/works/{id}", get(catalogue::get_work))
        .route("/v1/events", post(events::post_event))
        .route("/v1/me", get(users::me).delete(users::delete_me))
        .route("/v1/me/preferences", put(users::put_preferences))
        .route("/v1/me/export", get(users::export))
        .route("/v1/me/saved", get(saved::list))
        .route("/v1/me/saved/{card_id}", put(saved::save).delete(saved::unsave))
        .nest("/v1/admin", editorial::router());
    if state.config.dev_auth {
        // Serve the buildless local lab and allow other local development clients.
        app = app.merge(web::router()).layer(CorsLayer::very_permissive());
    }
    app.layer(axum::middleware::from_fn(error::normalize_errors))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "shirori-api" }))
}
