pub mod archive;
pub mod catalog;
pub mod checksum;
pub mod download;
pub mod fs;
pub mod install;
pub mod metadata_cache;
pub mod metadata_http;
pub mod process;
pub mod shell;
pub mod shim;
pub mod store;

pub fn adapters_ready() -> bool {
    true
}
