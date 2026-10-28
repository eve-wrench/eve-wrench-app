use std::collections::HashSet;
use std::path::PathBuf;

use eve_wrench_core::SettingsEntry;
use eve_wrench_core::confirmations::{self, ConfirmationKind};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _, WindowExt as _,
    h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;

use super::MainView;
use crate::ui::{self, StyleExt as _};

impl MainView {
    // Reads which dialogs the account suppresses, then opens the editor.
    pub(super) fn edit_confirmations(
        &mut self,
        entry: SettingsEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = PathBuf::from(&entry.path);
        cx.spawn_in(window, async move |this, cx| {
            let suppressed = cx
                .background_spawn({
                    let path = path.clone();
                    async move { confirmations::read_suppressed(&path) }
                })
                .await;
            this.update_in(cx, |this, window, cx| match suppressed {
                Ok(suppressed) => this.show_confirmations_dialog(entry, suppressed, window, cx),
                Err(e) => ui::notify_error(t!("toast.confirmationsLoadFailed"), e, window, cx),
            })
            .ok();
        })
        .detach();
    }

    fn show_confirmations_dialog(
        &mut self,
        entry: SettingsEntry,
        suppressed: HashSet<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = t!("confirmations.title", name = entry.display_name).to_string();
        let main = cx.entity().downgrade();
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t!("confirmations.search").to_string())
        });
        let dialog_view = cx.new(|cx| ConfirmationsDialog {
            path: PathBuf::from(entry.path),
            selected: suppressed.clone(),
            stored: suppressed,
            section: None,
            suppressed_only: false,
            expanded: HashSet::new(),
            main,
            _subscription: cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            search,
        });

        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title(title.clone())
                .w(px(900.))
                .child(dialog_view.clone())
        });
    }

    fn save_confirmations(
        &mut self,
        path: PathBuf,
        suppressed: HashSet<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let backup = self.store.read(cx).config().auto_backup;
        let count = suppressed.len();
        self.run_job(
            window,
            cx,
            move |_| confirmations::write_suppressed(&path, &suppressed, backup),
            move |_, result, window, cx| match result {
                Ok(()) => ui::notify_success(
                    t!("toast.confirmationsSaved"),
                    t!("toast.confirmationsSavedDesc", count = count),
                    window,
                    cx,
                ),
                Err(e) => ui::notify_error(t!("toast.confirmationsSaveFailed"), e, window, cx),
            },
        );
    }
}

// The sidebar's groups: the catalog's kinds, then suppressed dialogs the
// catalog doesn't know.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Kind(ConfirmationKind),
    Other,
}

const SECTIONS: [Section; 5] = [
    Section::Kind(ConfirmationKind::Question),
    Section::Kind(ConfirmationKind::Warning),
    Section::Kind(ConfirmationKind::Notice),
    Section::Kind(ConfirmationKind::Welcome),
    Section::Other,
];

impl Section {
    fn id(self) -> &'static str {
        match self {
            Section::Kind(ConfirmationKind::Question) => "question",
            Section::Kind(ConfirmationKind::Warning) => "warning",
            Section::Kind(ConfirmationKind::Notice) => "notice",
            Section::Kind(ConfirmationKind::Welcome) => "welcome",
            Section::Other => "other",
        }
    }

    fn label(self) -> String {
        t!(format!("confirmations.kind.{}", self.id())).to_string()
    }

    fn icon(self) -> IconName {
        match self {
            Section::Kind(ConfirmationKind::Question) => IconName::MessageCircleQuestionMark,
            Section::Kind(ConfirmationKind::Warning) => IconName::TriangleAlert,
            Section::Kind(ConfirmationKind::Notice) => IconName::Info,
            Section::Kind(ConfirmationKind::Welcome) => IconName::Sparkles,
            Section::Other => IconName::Inbox,
        }
    }
}

// The catalog has English and Chinese texts.
fn language() -> &'static str {
    if rust_i18n::locale().starts_with("zh") {
        "zh"
    } else {
        "en"
    }
}

// One dialog as the list shows it.
struct Row {
    key: String,
    section: Section,
    title: String,
    body: Option<String>,
    answer: Option<u8>,
}

struct ConfirmationsDialog {
    path: PathBuf,
    // Suppressed in the file, and after saving
    stored: HashSet<String>,
    selected: HashSet<String>,
    // None shows every group
    section: Option<Section>,
    suppressed_only: bool,
    // Rows showing their full text
    expanded: HashSet<String>,
    main: WeakEntity<MainView>,
    search: Entity<InputState>,
    _subscription: Subscription,
}

impl ConfirmationsDialog {
    // Dialogs without a known answer can't be suppressed from here, only
    // cleared and restored if the file already has them.
    fn rows(&self) -> Vec<Row> {
        let catalog = confirmations::catalog();
        let language = language();
        let mut rows: Vec<Row> = catalog
            .iter()
            .filter(|d| d.can_suppress() || self.stored.contains(&d.key))
            .map(|d| Row {
                key: d.key.clone(),
                section: Section::Kind(d.kind),
                title: d.title(language).to_string(),
                body: d.body(language).map(str::to_string),
                answer: d.answer,
            })
            .collect();
        rows.extend(
            self.stored
                .iter()
                .filter(|key| !catalog.iter().any(|d| &d.key == *key))
                .map(|key| Row {
                    key: key.clone(),
                    section: Section::Other,
                    title: key.clone(),
                    body: None,
                    answer: None,
                }),
        );
        rows.sort_by_cached_key(|r| r.title.to_lowercase());
        rows
    }

    fn set(&mut self, key: &str, on: bool, cx: &mut Context<Self>) {
        if on {
            self.selected.insert(key.to_string());
        } else {
            self.selected.remove(key);
        }
        cx.notify();
    }

    fn render_nav_item(
        &self,
        section: Option<Section>,
        rows: &[Row],
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = cx.theme();
        let active = self.section == section;
        let in_section: Vec<&Row> = rows
            .iter()
            .filter(|r| section.is_none_or(|s| r.section == s))
            .collect();
        let on = in_section
            .iter()
            .filter(|r| self.selected.contains(&r.key))
            .count();
        let (icon, label) = match section {
            Some(section) => (section.icon(), section.label()),
            None => (
                IconName::LayoutDashboard,
                t!("confirmations.all").to_string(),
            ),
        };
        let (fg, muted, hover, active_bg) = (
            theme.foreground,
            theme.muted_foreground,
            theme.muted.opacity(0.5),
            theme.muted,
        );

        h_flex()
            .id(SharedString::from(format!(
                "confirmations-nav-{}",
                section.map_or("all", |s| s.id())
            )))
            .gap_2p5()
            .px_2p5()
            .py_1p5()
            .rounded_md()
            .cursor_pointer()
            .text_sm()
            .map(|this| {
                if active {
                    this.bg(active_bg).text_color(fg).font_medium()
                } else {
                    this.text_color(muted).hover(move |s| s.bg(hover))
                }
            })
            .child(Icon::new(icon).small())
            .child(div().flex_1().min_w_0().truncate().child(label))
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("{on}/{}", in_section.len())),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.section = section;
                cx.notify();
            }))
    }

    fn render_sidebar(&self, rows: &[Row], cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let sections: Vec<Section> = SECTIONS
            .into_iter()
            .filter(|s| rows.iter().any(|r| r.section == *s))
            .collect();
        v_flex()
            .w(px(210.))
            .flex_none()
            .gap_0p5()
            .pr_3()
            .border_r_1()
            .border_color(cx.theme().border.opacity(0.6))
            .child(self.render_nav_item(None, rows, cx))
            .child(
                div()
                    .h(px(1.))
                    .my_1p5()
                    .mx_2p5()
                    .bg(cx.theme().border.opacity(0.5)),
            )
            .children(
                sections
                    .into_iter()
                    .map(|s| self.render_nav_item(Some(s), rows, cx)),
            )
    }

    // A flat row with a divider below; clicking it shows the full text.
    fn render_row(&self, row: &Row, cx: &mut Context<Self>) -> Stateful<Div> {
        let on = self.selected.contains(&row.key);
        let theme = cx.theme();
        let (fg, muted, divider, hover) = (
            theme.foreground,
            theme.muted_foreground,
            theme.border.opacity(0.5),
            theme.muted.opacity(0.3),
        );
        let answer = match row.answer {
            Some(6) => Some(t!("confirmations.answerYes").to_string()),
            Some(1) => Some(t!("confirmations.answerOk").to_string()),
            _ => None,
        };
        let (row_key, switch_key) = (row.key.clone(), row.key.clone());
        let expanded = self.expanded.contains(&row.key);
        // Collapsed rows show the text as one line
        let preview = row
            .body
            .as_ref()
            .map(|b| b.split_whitespace().collect::<Vec<_>>().join(" "));

        h_flex()
            .id(SharedString::from(format!("confirmation-{}", row.key)))
            .items_start()
            .gap_3()
            .px_2()
            .py_2p5()
            .border_b_1()
            .border_color(divider)
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .child(
                div().pt(px(2.)).child(
                    Icon::new(row.section.icon())
                        .small()
                        .text_color(muted.opacity(0.7)),
                ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .when(!expanded, |this| this.truncate())
                            .text_color(if on { fg } else { fg.opacity(0.85) })
                            .child(row.title.clone()),
                    )
                    .map(|this| match (expanded, row.body.clone(), preview) {
                        (true, Some(body), _) => {
                            this.child(div().text_xs().text_color(muted).child(body))
                        }
                        (false, _, Some(preview)) => {
                            this.child(div().text_xs().truncate().text_color(muted).child(preview))
                        }
                        _ => this,
                    })
                    .when(expanded, |this| {
                        this.child(
                            div()
                                .pt_1()
                                .text_xs()
                                .text_color(muted.opacity(0.6))
                                .child(row.key.clone()),
                        )
                    }),
            )
            .child(
                h_flex()
                    .flex_none()
                    .gap_3()
                    .when_some(answer.filter(|_| on), |this, answer| {
                        this.child(div().text_xs().text_color(muted).child(answer))
                    })
                    // The switch toggles; clicks elsewhere on the row expand it
                    .child(
                        div()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(
                                Switch::new(SharedString::from(format!("switch-{}", row.key)))
                                    .small()
                                    .checked(on)
                                    .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                        this.set(&switch_key, *checked, cx)
                                    })),
                            ),
                    )
                    .child(
                        Icon::new(if expanded {
                            IconName::ChevronUp
                        } else {
                            IconName::ChevronDown
                        })
                        .small()
                        .text_color(muted.opacity(0.6)),
                    ),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this.expanded.remove(&row_key) {
                    this.expanded.insert(row_key.clone());
                }
                cx.notify();
            }))
    }

    // Turns every row in view on or off: the group, search and filter decide
    // what that is.
    fn set_all(&mut self, keys: &[String], on: bool, cx: &mut Context<Self>) {
        for key in keys {
            if on {
                self.selected.insert(key.clone());
            } else {
                self.selected.remove(key);
            }
        }
        cx.notify();
    }

    fn render_list(&self, rows: &[Row], cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let query = self.search.read(cx).value().trim().to_lowercase();
        let visible: Vec<&Row> = rows
            .iter()
            .filter(|r| self.section.is_none_or(|s| r.section == s))
            .filter(|r| !self.suppressed_only || self.selected.contains(&r.key))
            .filter(|r| {
                query.is_empty()
                    || [Some(&r.key), Some(&r.title), r.body.as_ref()]
                        .into_iter()
                        .flatten()
                        .any(|text| text.to_lowercase().contains(&query))
            })
            .collect();
        let muted = cx.theme().muted_foreground;

        let toolbar = h_flex()
            .gap_2()
            .child(
                div().flex_1().child(
                    Input::new(&self.search)
                        .small()
                        .cleanable(true)
                        .prefix(Icon::new(IconName::Search).small().text_color(muted)),
                ),
            )
            .child(
                Button::new("confirmations-suppressed-only")
                    .outline()
                    .small()
                    .icon(IconName::BellOff)
                    .label(t!("confirmations.suppressedOnly").to_string())
                    .selected(self.suppressed_only)
                    .toggled(self.suppressed_only)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.suppressed_only = !this.suppressed_only;
                        cx.notify();
                    })),
            );

        let keys: Vec<String> = visible.iter().map(|r| r.key.clone()).collect();
        let on = keys.iter().filter(|k| self.selected.contains(*k)).count();
        let (on_keys, off_keys) = (keys.clone(), keys.clone());
        let bulk = h_flex()
            .gap_1()
            .px_2()
            .pb_1()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.5))
            .text_xs()
            .text_color(muted)
            .child(
                div()
                    .flex_1()
                    .child(t!("confirmations.shown", on = on, count = keys.len()).to_string()),
            )
            .child(
                Button::new("confirmations-all-on")
                    .ghost()
                    .xsmall()
                    .label(t!("confirmations.allOn").to_string())
                    .disabled(on == keys.len())
                    .on_click(cx.listener(move |this, _, _, cx| this.set_all(&on_keys, true, cx))),
            )
            .child(
                Button::new("confirmations-all-off")
                    .ghost()
                    .xsmall()
                    .label(t!("confirmations.allOff").to_string())
                    .disabled(on == 0)
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.set_all(&off_keys, false, cx)),
                    ),
            );

        let list = if visible.is_empty() {
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_2()
                .text_sm()
                .text_color(muted)
                .child(Icon::new(IconName::Search).large().text_color(muted))
                .child(t!("confirmations.empty").to_string())
                .into_any_element()
        } else {
            v_flex()
                .children(visible.into_iter().map(|r| self.render_row(r, cx)))
                .into_any_element()
        };

        v_flex().flex_1().min_w_0().gap_3().child(toolbar).child(
            v_flex().flex_1().min_h_0().child(bulk).child(
                div()
                    .id("confirmations-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .pr_1()
                    .child(list),
            ),
        )
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let changed = self.selected.symmetric_difference(&self.stored).count();
        let muted = cx.theme().muted_foreground;
        h_flex()
            .gap_2()
            .child(
                h_flex()
                    .flex_1()
                    .gap_2()
                    .text_xs()
                    .text_color(muted)
                    .child(Icon::new(IconName::BellOff).xsmall().text_color(muted))
                    .child(t!("confirmations.summary", count = self.selected.len()).to_string())
                    .when(changed > 0, |this| {
                        this.child(
                            div()
                                .px_1p5()
                                .rounded(px(4.))
                                .bg(ui::amber(cx).opacity(0.15))
                                .text_color(ui::amber(cx))
                                .child(t!("confirmations.changes", count = changed).to_string()),
                        )
                    }),
            )
            .child(
                Button::new("confirmations-cancel")
                    .ghost()
                    .label(t!("common.cancel").to_string())
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            )
            .child(
                Button::new("confirmations-save")
                    .primary()
                    .label(t!("confirmations.save").to_string())
                    .disabled(changed == 0)
                    .on_click(cx.listener(|this, _, window, cx| {
                        let path = this.path.clone();
                        let selected = this.selected.clone();
                        this.main
                            .update(cx, |main, cx| {
                                main.save_confirmations(path, selected, window, cx)
                            })
                            .ok();
                        window.close_dialog(cx);
                    })),
            )
    }
}

impl Render for ConfirmationsDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows();
        v_flex()
            .gap_4()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("confirmations.hint").to_string()),
            )
            .child(
                h_flex()
                    .items_stretch()
                    .gap_4()
                    // Fill the window, leaving room for the title, hint and footer
                    .h((window.viewport_size().height - px(280.)).clamp(px(320.), px(760.)))
                    .child(self.render_sidebar(&rows, cx))
                    .child(self.render_list(&rows, cx)),
            )
            .child(self.render_footer(cx))
    }
}
