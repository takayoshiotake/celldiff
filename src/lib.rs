pub mod compare;
pub mod console;
pub mod encoding;
pub mod input;
pub mod output;
mod quote_tracking;
pub mod text;

pub type AppError = Box<dyn std::error::Error + Send + Sync>;
