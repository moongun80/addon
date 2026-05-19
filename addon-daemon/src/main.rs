//! # addon-daemon
//!
//! Daemon binary for the addon — background service that watches and applies key bindings.

use anyhow::Result;
use tracing::info;

fn main() -> Result<()> {
    addon_core::log::init()?;
    info!("addon-daemon starting");
    // TODO: load config, select OS adapter, start listening.
    Ok(())
}
