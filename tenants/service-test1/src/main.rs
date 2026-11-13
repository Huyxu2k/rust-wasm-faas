use wstd::http::{Body, Request, Response, Result, StatusCode};
use wstd::time::Duration;

#[wstd::http_server]
async fn main(req: Request<Body>) -> Result<Response<Body>> {
    let path = req.uri().path().to_owned();

    match path.as_str() {
        "/" => text(StatusCode::OK,"Hello từ Wasm service test 1!\n"),
        "/timeout" => {
            wstd::task::sleep(Duration::from_secs(10)).await;
            text(StatusCode::OK, "Đã sleep 10 giây (mặc định)!\n")
        },
        p if p.starts_with("/timeout/") => {
            let seconds_str = &p["/timeout/".len()..];
            let seconds = seconds_str.parse::<u64>().unwrap_or(10);
            
            wstd::task::sleep(Duration::from_secs(seconds)).await;
            text(StatusCode::OK, format!("Đã sleep {} giây!\n", seconds))
        }
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