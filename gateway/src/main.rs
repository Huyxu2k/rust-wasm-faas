mod runtime_manager;
mod error;

use error::AppError;

pub type Result<T> = std::result::Result<T, AppError>;

fn main() {
    println!("Hello, world!");
}
