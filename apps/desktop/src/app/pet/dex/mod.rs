use super::*;

mod list;
mod overlay;
mod sidebar;
mod workspace;

use list::*;
use overlay::*;
use sidebar::*;

#[derive(Clone)]
pub(super) enum PetDexVirtualRow {
    Spacer {
        height: f32,
    },
    SectionHeader {
        label: String,
        trailing: Option<String>,
    },
    PetCardRow {
        cards: Vec<PetDexCard>,
        columns: usize,
    },
    EmptyState {
        message: String,
    },
    LegacyRow {
        record: Box<PetLegacyRecord>,
        sprite_path: ImageSource,
        language: String,
    },
}

#[derive(Clone)]
pub(super) enum PetDexCard {
    Bundled {
        item: PetCatalogItem,
        unlocked: bool,
        sprite_path: Option<ImageSource>,
        title: String,
        subtitle: String,
    },
    Custom {
        pet: PetCustomPet,
        sprite_path: ImageSource,
        subtitle: String,
    },
}

impl PetDexVirtualRow {
    pub(super) fn height(&self) -> gpui::Pixels {
        px(match self {
            PetDexVirtualRow::Spacer { height } => *height,
            PetDexVirtualRow::SectionHeader { .. } => 34.0,
            PetDexVirtualRow::PetCardRow { .. } => 148.0,
            PetDexVirtualRow::EmptyState { .. } => 84.0,
            PetDexVirtualRow::LegacyRow { .. } => 72.0,
        })
    }

    pub(super) fn render(
        &self,
        rows: &Rc<Vec<PetDexVirtualRow>>,
        index: usize,
        cx: &mut Context<CoduxApp>,
    ) -> gpui::Div {
        match self {
            PetDexVirtualRow::Spacer { .. } => div().w_full(),
            PetDexVirtualRow::SectionHeader { label, trailing } => div()
                .w_full()
                .px(px(20.0))
                .pt(px(8.0))
                .child(pet_section_header(label.clone(), trailing.clone())),
            PetDexVirtualRow::PetCardRow { cards, columns } => div()
                .w_full()
                .px(px(20.0))
                .pt(px(12.0))
                .flex()
                .gap(px(12.0))
                .children(
                    cards
                        .iter()
                        .cloned()
                        .map(|card| pet_dex_virtual_card(card, cx)),
                )
                .children(
                    (cards.len()..*columns)
                        .map(|_| div().flex_1().min_w_0().h(px(136.0)).into_any_element()),
                ),
            PetDexVirtualRow::EmptyState { message } => div()
                .w_full()
                .px(px(20.0))
                .pt(px(12.0))
                .child(pet_dex_empty_state(message.clone(), cx)),
            PetDexVirtualRow::LegacyRow {
                record,
                sprite_path,
                language,
            } => div()
                .w_full()
                .px(px(20.0))
                .pt(px(8.0))
                .child(pet_legacy_row(
                    record.as_ref().clone(),
                    sprite_path.clone(),
                    language.clone(),
                    cx,
                )),
        }
        .when(
            matches!(self, PetDexVirtualRow::LegacyRow { .. })
                && rows
                    .get(index + 1)
                    .map(|next| !matches!(next, PetDexVirtualRow::LegacyRow { .. }))
                    .unwrap_or(true),
            |this| this.mb(px(12.0)),
        )
    }
}

pub(super) fn pet_section_header(label: String, trailing: Option<String>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(rems(1.0))
                .line_height(rems(1.25))
                .font_weight(FontWeight::BOLD)
                .child(label),
        )
        .when_some(trailing, |this, trailing| {
            this.child(
                div()
                    .flex_none()
                    .text_size(rems(0.75))
                    .line_height(rems(1.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(color(theme::TEXT_MUTED))
                    .child(trailing),
            )
        })
}
