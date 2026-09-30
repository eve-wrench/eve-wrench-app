use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

// Lucide icons the app uses beyond the component library's default set.
gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        Archive,
        ArrowDownToLine,
        ArrowUpFromLine,
        ChevronsDown,
        ChevronsUp,
        Compass,
        Crosshair,
        Download,
        FilePlus,
        Grid2x2,
        Languages,
        Pencil,
        Radar,
        Rocket,
        RotateCcw,
        Save,
        Scale,
        Skull,
        Trash,
        Upload,
        Wrench,
        X,
    ]
);

pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
