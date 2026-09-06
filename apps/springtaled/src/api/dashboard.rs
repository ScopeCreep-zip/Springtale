use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

/// Dashboard SPA files embedded in the binary at compile time.
///
/// In debug builds (`cargo run`), files are loaded from the filesystem
/// at the specified path — changes are reflected without recompilation.
/// In release builds (`cargo build --release`), files are baked into
/// the binary — no external file dependencies needed.
///
/// This means:
/// - Development: edit dashboard files → refresh browser → see changes
/// - Production: single self-contained binary, works from any directory
/// - Docker: no volume mounts needed for dashboard assets
#[derive(Embed)]
#[folder = "../../tauri/apps/dashboard/dist/"]
struct DashboardAssets;

/// Serve an embedded dashboard file by path.
///
/// Falls back to index.html for SPA client-side routing.
/// Mount this at `/ui/` in the router.
#[utoipa::path(
    get, operation_id = "dashboard_serve_dashboard",
    path = "/ui/{path}",
    tag = "dashboard",
    params(("path" = String, Path, description = "Asset path under the SPA root")),
    responses((status = 200, description = "Dashboard asset", body = String))
)]
pub async fn serve_dashboard(axum::extract::Path(path): axum::extract::Path<String>) -> Response {
    serve_embedded_file(&path)
}

/// Serve the dashboard index page (root of /ui/).
#[utoipa::path(
    get, operation_id = "dashboard_serve_dashboard_index",
    path = "/ui",
    tag = "dashboard",
    security(()),
    responses((status = 200, description = "Dashboard SPA index", body = String))
)]
pub async fn serve_dashboard_index() -> Response {
    serve_embedded_file("index.html")
}

fn serve_embedded_file(path: &str) -> Response {
    // Try the requested path first, fall back to index.html for SPA routing
    let file = DashboardAssets::get(path).or_else(|| DashboardAssets::get("index.html"));

    match file {
        Some(content) => {
            let mime = mime_from_path(path);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime)],
                content.data.into_owned(),
            )
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Derive MIME type from file extension.
fn mime_from_path(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}
