//! `ffpr serve`: a local preview of `site/`.

use std::path::{Path, PathBuf};

use anyhow::Result;

/// Serve `root` on `port` until interrupted.
pub fn serve(root: &Path, port: u16) -> Result<()> {
    let server = tiny_http::Server::http(("0.0.0.0", port))
        .map_err(|e| anyhow::anyhow!("cannot listen on port {port}: {e}"))?;
    for request in server.incoming_requests() {
        let url_path = request
            .url()
            .split(['?', '#'])
            .next()
            .unwrap_or("/")
            .to_string();
        let response = match resolve(root, &url_path) {
            Some(file) => match std::fs::read(&file) {
                Ok(bytes) => {
                    let header =
                        tiny_http::Header::from_bytes(&b"Content-Type"[..], content_type(&file))
                            .expect("static header");
                    tiny_http::Response::from_data(bytes)
                        .with_header(header)
                        .boxed()
                }
                Err(_) => tiny_http::Response::from_string("not found")
                    .with_status_code(404)
                    .boxed(),
            },
            None => tiny_http::Response::from_string("not found")
                .with_status_code(404)
                .boxed(),
        };
        let _ = request.respond(response);
    }
    Ok(())
}

/// The file a URL path names, with directory URLs mapped to `index.html`.
/// Paths that try to leave `root` are refused.
fn resolve(root: &Path, url_path: &str) -> Option<PathBuf> {
    let mut path = root.to_path_buf();
    for part in url_path.trim_start_matches('/').split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            name if name.contains(['\\', ':']) => return None,
            name => path.push(name),
        }
    }
    if path.is_dir() {
        path.push("index.html");
    }
    path.is_file().then_some(path)
}

fn content_type(path: &Path) -> &'static [u8] {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => b"text/html; charset=utf-8",
        Some("css") => b"text/css; charset=utf-8",
        Some("js") => b"application/javascript; charset=utf-8",
        Some("json") => b"application/json; charset=utf-8",
        Some("svg") => b"image/svg+xml",
        Some("png") => b"image/png",
        Some("jpg") | Some("jpeg") => b"image/jpeg",
        _ => b"application/octet-stream",
    }
}
