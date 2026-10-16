pub mod compare;
pub mod encoding;
pub mod input;
pub mod output;
pub mod text;

pub type AppError = Box<dyn std::error::Error + Send + Sync>;
