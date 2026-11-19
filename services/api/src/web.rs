//! Buildless local test interface, exposed only with development authentication enabled.
use crate::AppState;
use axum::{Router, http::header, routing::get};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(|| async { asset("text/html; charset=utf-8", include_str!("../../../apps/web/index.html")) }),
        )
        .route(
            "/lab/app.js",
            get(|| async {
                asset(
                    "text/javascript; charset=utf-8",
                    include_str!("../../../apps/web/app.js"),
                )
            }),
        )
        .route(
            "/lab/style.css",
            get(|| async { asset("text/css; charset=utf-8", include_str!("../../../apps/web/style.css")) }),
        )
}

fn asset(content_type: &'static str, content: &'static str) -> ([(header::HeaderName, &'static str); 3], &'static str) {
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        content,
    )
}
