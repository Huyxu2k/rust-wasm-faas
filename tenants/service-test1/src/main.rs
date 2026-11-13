
use wstd::http::{Body, Request, Response, Result, StatusCode};

#[wstd::http_server]
async fn main(req: Request<Body>) -> Result<Response<Body>> {
    let path = req.uri().path().to_owned();

    match path.as_str() {
        "/" => text(StatusCode::OK,"Hello từ Wasm service test 1!\n"),
        p if p.starts_with("/hello/") => {
            let name = &p["/hello/".len()..];
            text(StatusCode::OK, format!("Xin chào: {}!\n", name))
        }
        _ => text(StatusCode::NOT_FOUND, "Không tìm thấy\n")
    }
}

fn text(status: StatusCode, body: impl Into<String>) -> Result<Response<Body>> {
    Ok(Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(body.into().into())?)
}