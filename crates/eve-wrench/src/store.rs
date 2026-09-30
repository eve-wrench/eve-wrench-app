use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use eve_wrench_core::config::Config;
use eve_wrench_core::{AppData, CharacterDetails, Locations, SettingsKind, esi, scan};
use gpui_kit::*;

pub enum StoreEvent {
    DataChanged,
    LoadFailed(SharedString),
}

// App-wide state shared by every window: the scanned settings files and the
// persisted config. Data is behind an `Rc` so views can hold it across a
// render without copying. Any change on disk goes through `reload`, which emits
// `DataChanged` so open windows (e.g. formation editors) can catch up.
pub struct Store {
    locations: Locations,
    config: Config,
    data: Option<Rc<AppData>>,
    // Decoded once per character so rows don't rebuild images every frame
    portraits: HashMap<String, Arc<Image>>,
    loading: bool,
    _load: Option<Task<()>>,
    editor_windows: HashMap<String, AnyWindowHandle>,
}

struct GlobalStore(Entity<Store>);

impl Global for GlobalStore {}

impl EventEmitter<StoreEvent> for Store {}

impl Store {
    pub fn init(cx: &mut App) -> Entity<Store> {
        let data_dir = Locations::default_data_dir();
        let config = Config::load(&Locations::new(data_dir.clone(), None).config_file());
        let locations =
            Locations::new(data_dir, config.custom_eve_path.as_ref().map(PathBuf::from));
        let store = cx.new(|cx| {
            let mut store = Store {
                locations,
                config,
                data: None,
                portraits: HashMap::new(),
                loading: false,
                _load: None,
                editor_windows: HashMap::new(),
            };
            store.reload(cx);
            store
        });
        cx.set_global(GlobalStore(store.clone()));
        store
    }

    pub fn global(cx: &App) -> Entity<Store> {
        cx.global::<GlobalStore>().0.clone()
    }

    pub fn locations(&self) -> &Locations {
        &self.locations
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn data(&self) -> Option<Rc<AppData>> {
        self.data.clone()
    }

    pub fn portrait(&self, character_id: &str) -> Option<Arc<Image>> {
        self.portraits.get(character_id).cloned()
    }

    fn set_data(&mut self, data: AppData) {
        for entry in data.entries() {
            if let Some(jpeg) = entry.character.as_ref().and_then(|c| c.portrait.as_ref())
                && !self.portraits.contains_key(&entry.id)
            {
                let image = Image::from_bytes(ImageFormat::Jpeg, jpeg.to_vec());
                self.portraits.insert(entry.id.clone(), Arc::new(image));
            }
        }
        self.data = Some(Rc::new(data));
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        cx.notify();

        let locations = self.locations.clone();
        self._load = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { scan::load_app_data(&locations) })
                .await;

            let Ok(ids) = this.update(cx, |this, cx| {
                this.loading = false;
                let ids = match result {
                    Ok(mut data) => {
                        // Reuse names already resolved so a reload doesn't
                        // flash raw character IDs while ESI is queried again
                        data.apply_character_details(&this.known_characters());
                        let ids = data.esi_character_ids();
                        this.set_data(data);
                        cx.emit(StoreEvent::DataChanged);
                        ids
                    }
                    Err(error) => {
                        if this.data.is_none() {
                            this.set_data(AppData::default());
                        }
                        cx.emit(StoreEvent::LoadFailed(error.into()));
                        Vec::new()
                    }
                };
                cx.notify();
                ids
            }) else {
                return;
            };
            if ids.is_empty() {
                return;
            }

            let details = cx
                .background_spawn(async move { esi::fetch_character_details(&ids) })
                .await;
            this.update(cx, |this, cx| {
                if let Some(data) = &this.data {
                    let mut data = AppData::clone(data);
                    data.apply_character_details(&details);
                    this.set_data(data);
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    fn known_characters(&self) -> HashMap<i64, CharacterDetails> {
        self.data
            .iter()
            .flat_map(|data| data.entries())
            .filter(|e| e.kind == SettingsKind::Char)
            .filter_map(|e| Some((e.id.parse().ok()?, e.character.clone()?)))
            .collect()
    }

    pub fn update_config(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut Config),
    ) -> Result<(), String> {
        let previous_path = self.config.custom_eve_path.clone();
        edit(&mut self.config);
        let saved = self.config.save(&self.locations.config_file());

        if self.config.custom_eve_path != previous_path {
            self.locations = Locations::new(
                self.locations.data_dir().to_path_buf(),
                self.config.custom_eve_path.as_ref().map(PathBuf::from),
            );
            self.reload(cx);
        }
        cx.notify();
        saved
    }

    pub fn editor_window(&self, path: &str) -> Option<AnyWindowHandle> {
        self.editor_windows.get(path).copied()
    }

    pub fn register_editor_window(&mut self, path: String, window: AnyWindowHandle) {
        self.editor_windows.insert(path, window);
    }
}
