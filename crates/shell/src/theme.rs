//! RevuCraft's themes, for PlanCraft's window: a light or charcoal chrome, the same as RevuCraft's,
//! and one accent colour the person picks (amber unless changed). Each shell colour is taken from
//! RevuCraft's palette and written into the shared `Tokens` the shell draws with, so the window
//! reads as one family with RevuCraft.

use egui::Color32;
use gridcraft_ui_egui::theme::Tokens;

/// The accent colour for selections, the active tab and the primary buttons. Each has a light and
/// a dark tuning, as RevuCraft's do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Accent {
    #[default]
    Amber,
    Blue,
    Teal,
    Green,
    Red,
    Violet,
}

impl Accent {
    pub const ALL: [Accent; 6] = [Accent::Amber, Accent::Blue, Accent::Teal, Accent::Green, Accent::Red, Accent::Violet];

    pub fn label(self) -> &'static str {
        match self {
            Self::Amber => "Amber",
            Self::Blue => "Blue",
            Self::Teal => "Teal",
            Self::Green => "Green",
            Self::Red => "Red",
            Self::Violet => "Violet",
        }
    }

    /// (accent, accent text, accent soft, text on the accent) for the given theme.
    fn colours(self, dark: bool) -> (Color32, Color32, Color32, Color32) {
        let rgb = |v: u32| Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
        let (a, text, soft, on) = match (self, dark) {
            (Self::Amber, true) => (0xF2A33A, 0xF5BA63, 0x3D2E17, 0x1A1205),
            (Self::Amber, false) => (0xC77700, 0x8F5300, 0xFCEBD0, 0x1A1205),
            (Self::Blue, true) => (0x4B8BF5, 0x8CB6FA, 0x243657, 0xFFFFFF),
            (Self::Blue, false) => (0x1B63E0, 0x1755C4, 0xE3ECFD, 0xFFFFFF),
            (Self::Teal, true) => (0x2BB3A3, 0x62D4C5, 0x173A37, 0x06201D),
            (Self::Teal, false) => (0x0F7F73, 0x0B6B61, 0xDDF3F0, 0xFFFFFF),
            (Self::Green, true) => (0x4CB772, 0x80D49C, 0x1B3A27, 0x07200F),
            (Self::Green, false) => (0x2E7D46, 0x256B3B, 0xE1F2E6, 0xFFFFFF),
            (Self::Red, true) => (0xE5534B, 0xF28C86, 0x43201E, 0xFFFFFF),
            (Self::Red, false) => (0xC62F28, 0xB0251F, 0xFBE3E1, 0xFFFFFF),
            (Self::Violet, true) => (0x9B7BF0, 0xBCA6F7, 0x2E2552, 0xFFFFFF),
            (Self::Violet, false) => (0x6B4BD8, 0x5A3BC4, 0xECE6FD, 0xFFFFFF),
        };
        (rgb(a), rgb(text), rgb(soft), rgb(on))
    }
}

/// RevuCraft's chrome colours for one theme: the title strip, the panels, the charcoal ground
/// and the text.
struct Chrome {
    titlebar: Color32,
    chrome: Color32,
    panel: Color32,
    pasteboard: Color32,
    card: Color32,
    border: Color32,
    divider: Color32,
    text: Color32,
    text_muted: Color32,
    text_faint: Color32,
    hover: Color32,
    pressed: Color32,
    field: Color32,
    icon: Color32,
}

fn chrome(dark: bool) -> Chrome {
    let c = |v: u32| Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
    if dark {
        Chrome {
            titlebar: c(0x16181B),
            chrome: c(0x1E2024),
            panel: c(0x1A1C20),
            pasteboard: c(0x2A2D32),
            card: c(0x24272C),
            border: c(0x3A3F46),
            divider: c(0x2E3238),
            text: c(0xE6E8EC),
            text_muted: c(0xAEB4BD),
            text_faint: c(0x9AA0AA),
            hover: c(0x2B2F35),
            pressed: c(0x353A41),
            field: c(0x16181B),
            icon: c(0xCDD1D8),
        }
    } else {
        Chrome {
            titlebar: c(0xE9E9EB),
            chrome: c(0xFFFFFF),
            panel: c(0xFFFFFF),
            pasteboard: c(0xF1F1F3),
            card: c(0xFFFFFF),
            border: c(0xDADADE),
            divider: c(0xE8E8EB),
            text: c(0x222226),
            text_muted: c(0x5E5E66),
            text_faint: c(0x6B6B73),
            hover: c(0xF0F0F3),
            pressed: c(0xE4E4E9),
            field: c(0xFFFFFF),
            icon: c(0x44444B),
        }
    }
}

/// The tokens for a theme and accent: the shared palette, with RevuCraft's chrome and accent.
pub fn tokens_for(dark: bool, accent: Accent) -> Tokens {
    let base = if dark { Tokens::dark() } else { Tokens::light() };
    let c = chrome(dark);
    let (a, text, soft, on) = accent.colours(dark);
    Tokens {
        window: c.panel,
        ribbon: c.chrome,
        ribbon_border: c.border,
        text: c.text,
        text_dim: c.text_muted,
        text_disabled: c.text_faint,
        accent: a,
        accent_dark: text,
        accent_soft: soft,
        hover: c.hover,
        pressed: c.pressed,
        separator: c.divider,
        desk: c.pasteboard,
        grid_bg: c.panel,
        cell_text: c.text,
        header_bg: c.card,
        menu_bg: c.card,
        input_bg: c.field,
        input_border: c.border,
        tab_bar: c.titlebar,
        tab_active: c.chrome,
        status_bar: c.chrome,
        sel_border: a,
        sel_fill: soft,
        on_accent: on,
        ..base
    }
}

#[allow(deprecated)] // a fixed key for this context's data, not a salt
fn accent_id() -> egui::Id {
    egui::Id::new("plancraft-accent")
}

/// What the tool icons are drawn in (RevuCraft's choice): the accent, a neutral grey, or the colour
/// of one of the accents, whatever the accent is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IconColor {
    #[default]
    Accent,
    Neutral,
    Own(Accent),
}

impl IconColor {
    /// Every choice, for the menu: the accent, neutral, then each colour.
    pub fn all() -> Vec<IconColor> {
        let mut out = vec![IconColor::Accent, IconColor::Neutral];
        out.extend(Accent::ALL.into_iter().map(IconColor::Own));
        out
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Accent => "Accent colour",
            Self::Neutral => "Neutral",
            Self::Own(a) => a.label(),
        }
    }
}

/// The icon colour for a theme, accent and choice.
pub fn tool_icon_for(dark: bool, accent: Accent, icons: IconColor) -> Color32 {
    match icons {
        IconColor::Neutral => chrome(dark).icon,
        IconColor::Accent | IconColor::Own(_) => {
            let pick = if let IconColor::Own(a) = icons { a } else { accent };
            let (a, text, ..) = pick.colours(dark);
            if dark { a } else { text }
        }
    }
}

#[allow(deprecated)] // a fixed key for this context's data, not a salt
fn icon_id() -> egui::Id {
    egui::Id::new("plancraft-tool-icons")
}

/// Remember the accent and the tool icon colour for this context; the shell's tokens use them.
pub fn remember(ctx: &egui::Context, accent: Accent, icons: IconColor) {
    ctx.data_mut(|d| {
        d.insert_temp(accent_id(), accent);
        d.insert_temp(icon_id(), icons);
    });
}

/// The colour the tool icons are drawn in now (ribbon, rail and tab-bar icons).
pub fn tool_icon(ctx: &egui::Context) -> Color32 {
    let dark = ctx.global_style().visuals.dark_mode;
    let accent = ctx.data(|d| d.get_temp::<Accent>(accent_id())).unwrap_or_default();
    let icons = ctx.data(|d| d.get_temp::<IconColor>(icon_id())).unwrap_or_default();
    tool_icon_for(dark, accent, icons)
}

/// The tokens to draw with now: the light or dark theme the context shows, with the remembered accent.
pub fn tokens(ctx: &egui::Context) -> Tokens {
    let dark = ctx.global_style().visuals.dark_mode;
    let accent = ctx.data(|d| d.get_temp::<Accent>(accent_id())).unwrap_or_default();
    tokens_for(dark, accent)
}
