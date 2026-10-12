//! `ffpr serve`: a local preview of `site/`, answering the way Python's
//! `http.server.SimpleHTTPRequestHandler` (what the Python version used) does.

use std::path::{Path, PathBuf};

use anyhow::Result;
use tiny_http::{Header, Method, Response, ResponseBox};

/// Serve `root` on `port` until interrupted.
pub fn serve(root: &Path, port: u16) -> Result<()> {
    let server = tiny_http::Server::http(("0.0.0.0", port))
        .map_err(|e| anyhow::anyhow!("cannot listen on port {port}: {e}"))?;
    println!(
        "Serving {}/ at http://localhost:{port}/ (Ctrl+C to stop)",
        root.display()
    );
    for request in server.incoming_requests() {
        let response = respond(root, request.method(), request.url());
        let _ = request.respond(response);
    }
    Ok(())
}

/// The response for one request.
///
/// A directory asked for without its trailing slash is redirected to the
/// slashed URL, so the page's relative links (`data.js`, `static/...`,
/// `weeks/...`) resolve inside that directory: `/2025` must not serve the
/// 2025 index against the root's `data.js`.
pub fn respond(root: &Path, method: &Method, url: &str) -> ResponseBox {
    if !matches!(method, Method::Get | Method::Head) {
        return text(501, "Unsupported method");
    }
    let without_fragment = url.split('#').next().unwrap_or("");
    let (raw_path, query) = match without_fragment.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (without_fragment, None),
    };
    let Some(path) = resolve(root, &percent_decode(raw_path)) else {
        return text(404, "File not found");
    };
    if path.is_dir() {
        if !raw_path.ends_with('/') {
            let location = match query {
                Some(q) => format!("{raw_path}/?{q}"),
                None => format!("{raw_path}/"),
            };
            let header =
                Header::from_bytes(&b"Location"[..], location.as_bytes()).expect("location header");
            return Response::empty(301).with_header(header).boxed();
        }
        return file(&path.join("index.html"));
    }
    if raw_path.ends_with('/') {
        // A file named like a directory: http.server answers 404.
        return text(404, "File not found");
    }
    file(&path)
}

fn file(path: &Path) -> ResponseBox {
    match std::fs::read(path) {
        Ok(bytes) => {
            let header = Header::from_bytes(&b"Content-Type"[..], content_type(path))
                .expect("static header");
            Response::from_data(bytes).with_header(header).boxed()
        }
        Err(_) => text(404, "File not found"),
    }
}

fn text(status: u16, body: &str) -> ResponseBox {
    Response::from_string(body).with_status_code(status).boxed()
}

/// The path a decoded URL path names under `root`. Paths that try to leave
/// `root` are refused.
fn resolve(root: &Path, url_path: &str) -> Option<PathBuf> {
    let mut path = root.to_path_buf();
    for part in url_path.trim_start_matches('/').split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            name if name.contains(['\\', ':', '\0']) => return None,
            name => path.push(name),
        }
    }
    path.exists().then_some(path)
}

/// `%XX` escapes decoded, as `urllib.parse.unquote` does for a request path.
fn percent_decode(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (
            bytes[i],
            bytes.get(i + 1).copied().and_then(hex),
            bytes.get(i + 2).copied().and_then(hex),
        ) {
            (b'%', Some(hi), Some(lo)) => {
                out.push((hi * 16 + lo) as u8);
                i += 3;
            }
            (b, _, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn site() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("2025/weeks")).unwrap();
        std::fs::write(dir.path().join("index.html"), "root").unwrap();
        std::fs::write(dir.path().join("2025/index.html"), "2025").unwrap();
        std::fs::write(dir.path().join("a b.txt"), "spaced").unwrap();
        dir
    }

    fn status(root: &Path, method: Method, url: &str) -> (u16, Option<String>) {
        let response = respond(root, &method, url);
        let location = response
            .headers()
            .iter()
            .find(|h| h.field.equiv("Location"))
            .map(|h| h.value.to_string());
        (response.status_code().0, location)
    }

    #[test]
    fn a_directory_without_its_slash_redirects_so_relative_links_work() {
        let dir = site();
        assert_eq!(
            status(dir.path(), Method::Get, "/2025"),
            (301, Some("/2025/".into()))
        );
        assert_eq!(
            status(dir.path(), Method::Get, "/2025?x=1"),
            (301, Some("/2025/?x=1".into()))
        );
        assert_eq!(status(dir.path(), Method::Get, "/2025/").0, 200);
        assert_eq!(status(dir.path(), Method::Get, "/").0, 200);
    }

    #[test]
    fn paths_are_percent_decoded_and_kept_inside_the_site() {
        let dir = site();
        assert_eq!(status(dir.path(), Method::Get, "/a%20b.txt").0, 200);
        assert_eq!(status(dir.path(), Method::Get, "/../etc/passwd").0, 404);
        assert_eq!(status(dir.path(), Method::Get, "/%2e%2e/etc/passwd").0, 404);
        assert_eq!(status(dir.path(), Method::Get, "/index.html/").0, 404);
        assert_eq!(status(dir.path(), Method::Get, "/missing.html").0, 404);
    }

    #[test]
    fn only_get_and_head_are_served() {
        let dir = site();
        assert_eq!(status(dir.path(), Method::Head, "/index.html").0, 200);
        assert_eq!(status(dir.path(), Method::Post, "/index.html").0, 501);
    }
}
