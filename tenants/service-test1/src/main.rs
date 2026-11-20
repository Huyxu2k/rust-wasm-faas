use wstd::http::{Body, Request, Response, Result, StatusCode};
use wstd::time::Duration;
use wit_bindgen::generate;

generate!({
    path: "../../wit/faas.wit",
    world: "faas",
});

use faas::platform::logging::{Level, log};

use faas::platform::sqlite;
use faas::platform::sqlite::Connection;

#[wstd::http_server]
async fn main(req: Request<Body>) -> Result<Response<Body>> {
    let path = req.uri().path().to_owned();
    let conn = match sqlite::open() {
        Ok(conn) => conn,
        Err(e) => {
            log(Level::Error, &e);
            return text(StatusCode::INTERNAL_SERVER_ERROR, "Database unavailable\n");
        }
    };

    match init(conn) {
        Ok(v) => log(Level::Info, &v.to_string()),
        Err(e) => log(Level::Error, &e),
    }

    match path.as_str() {
        "/" => text(StatusCode::OK,"Hello từ Wasm service test 1!\n"),
        "/timeout" => {
            log(Level::Info, "Đang chờ 10 giây...");
            wstd::task::sleep(Duration::from_secs(10)).await;
            text(StatusCode::OK, "Đã sleep 10 giây (mặc định)!\n")
        },
        p if p.starts_with("/timeout/") => {
            let seconds_str = &p["/timeout/".len()..];
            let seconds = seconds_str.parse::<u64>().unwrap_or(10);
            log(Level::Info, &format!("Đang chờ {} giây...", seconds).to_string());
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

fn init(conn: Connection) -> std::result::Result<u64, String> {
    // tạo bảng test
    conn.execute("CREATE TABLE IF NOT EXISTS test (id INTEGER PRIMARY KEY, name TEXT)", &[])
}