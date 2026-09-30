// Update checks and in-place installs. The release notice comes from the
// GitHub releases API (see `eve_wrench_core::updates`); installing needs a
// signed bundle listed in the release's `latest.json`, verified against the
// public key baked into release builds.

use cargo_packager_updater::{Config, Update, UpdaterBuilder};
use eve_wrench_core::updates::{self, APP_VERSION, UpdateInfo};

// Set by CI from the updater signing key; local builds have none and fall
// back to linking to the release page.
const PUBLIC_KEY: Option<&str> = option_env!("EVE_WRENCH_UPDATER_PUBKEY");

const MANIFEST_URL: &str =
    "https://github.com/eve-wrench/eve-wrench-app/releases/latest/download/latest.json";

pub struct AvailableUpdate {
    pub info: UpdateInfo,
    // Present when this platform can install the update in place
    pub installer: Option<Update>,
}

// Blocking; run off the UI thread.
pub fn check() -> Result<Option<AvailableUpdate>, String> {
    let Some(info) = updates::check_for_update()? else {
        return Ok(None);
    };
    let installer = installer_for(&info.latest_version);
    Ok(Some(AvailableUpdate { info, installer }))
}

// Only offers an install when the signed bundle is the same version the
// notice describes (the manifest tracks the latest stable release, while
// preview builds are also told about newer previews).
fn installer_for(version: &str) -> Option<Update> {
    let pubkey = PUBLIC_KEY.filter(|key| !key.is_empty())?;
    let config = Config {
        endpoints: vec![MANIFEST_URL.parse().ok()?],
        pubkey: pubkey.to_string(),
        ..Default::default()
    };
    let update = UpdaterBuilder::new(APP_VERSION.parse().ok()?, config)
        .build()
        .ok()?
        .check()
        .ok()??;
    (update.version.trim_start_matches('v') == version).then_some(update)
}

// Downloads, verifies the signature and installs. On Windows the installer
// takes over and relaunches the app, so this does not return there.
pub fn install(update: &Update) -> Result<(), String> {
    update.download_and_install().map_err(|e| format!("{}", e))
}
