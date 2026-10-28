use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

// Lucide icons the app uses beyond the component library's default set.
gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        AppWindow,
        Archive,
        ArrowDownToLine,
        ArrowLeftRight,
        ArrowUpFromLine,
        BellOff,
        Bookmark,
        Building2,
        ChevronsDown,
        ChevronsUp,
        Compass,
        Crosshair,
        Download,
        Drone,
        Eye,
        EyeOff,
        FilePlus,
        FileText,
        Gauge,
        GraduationCap,
        Grid2x2,
        Inbox,
        Info,
        Languages,
        Layers,
        LayoutDashboard,
        ListFilter,
        Lock,
        Magnet,
        Map,
        MapPin,
        Maximize,
        MessageCircle,
        MessageCircleQuestionMark,
        MessageSquare,
        Minus,
        Moon,
        Package,
        PanelsTopLeft,
        Pencil,
        Plus,
        Radar,
        Rocket,
        RotateCcw,
        Save,
        Scale,
        ScanSearch,
        Settings,
        ShoppingCart,
        Skull,
        Sparkles,
        Store,
        Trash,
        TriangleAlert,
        Upload,
        User,
        Users,
        Wallet,
        Warehouse,
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
