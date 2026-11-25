//! Source loading, symbolic entry execution and business PERS projection.

pub mod error;
pub mod formatting;
pub mod kernel_input;
pub mod normalized;
mod preprocess;

mod profile;
mod runtime_heap;
pub mod stages;
mod yaml_input;

pub use error::PipelineError;
pub use formatting::sql::{
    format_expr_sql, format_expr_with_ctx, FormatCtx,
};
pub use stages::load::parse_source_directory;


pub mod normalized_text;
