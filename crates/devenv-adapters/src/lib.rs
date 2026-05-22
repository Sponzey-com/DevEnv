pub mod archive;
pub mod checksum;
pub mod download;
pub mod fs;
pub mod install;
pub mod process;
pub mod shell;
pub mod shim;
pub mod store;

pub fn adapters_ready() -> bool {
    true
}
