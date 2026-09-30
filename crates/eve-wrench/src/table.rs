// Column layout shared by a table's header and its rows. Each table declares
// its columns once and renders every line through `cells`, so headers and
// rows can't drift out of alignment.

use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Start,
    End,
}

#[derive(Clone, Copy)]
pub struct Column {
    width: Option<Pixels>,
    align: Align,
}

impl Column {
    pub const fn fixed(width: Pixels) -> Self {
        Self {
            width: Some(width),
            align: Align::Start,
        }
    }

    pub const fn fill() -> Self {
        Self {
            width: None,
            align: Align::Start,
        }
    }

    pub const fn end(mut self) -> Self {
        self.align = Align::End;
        self
    }
}

pub fn cells<E: ParentElement>(
    line: E,
    columns: &[Column],
    cells: impl IntoIterator<Item = AnyElement>,
) -> E {
    line.children(
        columns
            .iter()
            .zip(cells)
            .map(|(column, content)| cell(column, content)),
    )
}

pub fn cell(column: &Column, content: impl IntoElement) -> Div {
    h_flex()
        .h_full()
        .min_w_0()
        .map(|this| match column.width {
            Some(width) => this.w(width).flex_none(),
            None => this.flex_1(),
        })
        .when(column.align == Align::End, |this| this.justify_end())
        .child(content)
}

pub fn empty() -> AnyElement {
    div().into_any_element()
}
