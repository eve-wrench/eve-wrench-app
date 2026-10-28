// Game data generated in CI (drone presets, confirmation dialogs): bundled into
// the app and refreshed from the repository at runtime, so a new game build
// reaches users without a release. Each file carries a schema, which guards
// older app versions against a format they can't read, and the game build it
// came from, which decides whether a download is newer.

use std::fs;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::esi::AGENT;

const REMOTE_BASE: &str =
    "https://raw.githubusercontent.com/eve-wrench/eve-wrench-app/main/crates/eve-wrench-core/data";

pub(crate) struct RemoteCatalog<T: 'static> {
    pub name: &'static str,
    pub bundled: &'static str,
    // Name in the repository's data directory and in the app's cache
    pub file: &'static str,
    pub schema: u32,
    pub build_field: &'static str,
    pub items_field: &'static str,
    current: OnceLock<RwLock<(u64, Arc<[T]>)>>,
}

impl<T: DeserializeOwned + Send + Sync> RemoteCatalog<T> {
    pub const fn new(
        name: &'static str,
        bundled: &'static str,
        file: &'static str,
        schema: u32,
        build_field: &'static str,
        items_field: &'static str,
    ) -> Self {
        Self {
            name,
            bundled,
            file,
            schema,
            build_field,
            items_field,
            current: OnceLock::new(),
        }
    }

    pub fn parse(&self, text: &str) -> Result<(u64, Arc<[T]>), String> {
        let name = self.name;
        let mut file: Value =
            serde_json::from_str(text).map_err(|e| format!("Invalid {name}: {e}"))?;
        let schema = file.get("schema").and_then(Value::as_u64);
        if schema != Some(u64::from(self.schema)) {
            return Err(format!("Unsupported {name} schema {schema:?}"));
        }
        let build = file
            .get(self.build_field)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("{name} has no build number"))?;
        let items: Vec<T> = serde_json::from_value(file[self.items_field].take())
            .map_err(|e| format!("Invalid {name}: {e}"))?;
        if items.is_empty() {
            return Err(format!("{name} is empty"));
        }
        Ok((build, items.into()))
    }

    fn current(&self) -> &RwLock<(u64, Arc<[T]>)> {
        self.current.get_or_init(|| {
            RwLock::new(self.parse(self.bundled).expect("bundled catalog is valid"))
        })
    }

    pub fn items(&self) -> Arc<[T]> {
        let current = self.current().read().unwrap_or_else(|e| e.into_inner());
        current.1.clone()
    }

    // Takes over a catalog built from a newer game build than the one in use.
    fn install(&self, catalog: (u64, Arc<[T]>)) -> bool {
        let mut current = self.current().write().unwrap_or_else(|e| e.into_inner());
        if catalog.0 <= current.0 {
            return false;
        }
        *current = catalog;
        true
    }

    // The copy downloaded on an earlier run, when it is newer than the one
    // built into this version.
    pub fn load_cached(&self, data_dir: &Path) {
        if let Some(catalog) = fs::read_to_string(data_dir.join(self.file))
            .ok()
            .and_then(|text| self.parse(&text).ok())
        {
            self.install(catalog);
        }
    }

    // Fetches the latest copy and caches it when it is newer. Returns whether
    // the catalog changed.
    pub fn refresh(&self, data_dir: &Path) -> Result<bool, String> {
        let name = self.name;
        let text = AGENT
            .get(format!("{REMOTE_BASE}/{}", self.file))
            .call()
            .map_err(|e| format!("Failed to fetch {name}: {e}"))?
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("Failed to read {name}: {e}"))?;
        if !self.install(self.parse(&text)?) {
            return Ok(false);
        }
        fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
        fs::write(data_dir.join(self.file), text).map_err(|e| e.to_string())?;
        Ok(true)
    }
}
