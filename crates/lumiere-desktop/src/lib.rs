//! Native filesystem, workers, persistence and platform services.
pub mod file_ops;
pub mod fsmodel;
#[cfg(target_arch = "wasm32")]
compile_error!("lumiere-desktop requires a native target; web support is a separate phase");

pub mod imaging;
pub mod loader;
pub mod menu_model;
pub mod platform;
pub mod state;
mod trash_names;
pub use trash_names::trash_names;
