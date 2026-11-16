//! Colours, egui style and fonts (dark theme in the spirit of CL Viewer).

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke};
use std::path::PathBuf;
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(22, 23, 26);
pub const PANEL: Color32 = Color32::from_rgb(28, 29, 33);
pub const BAR: Color32 = Color32::from_rgb(33, 34, 39);
pub const CARD: Color32 = Color32::from_rgb(36, 38, 43);
pub const CARD_HI: Color32 = Color32::from_rgb(44, 46, 53);
pub const BORDER: Color32 = Color32::from_rgb(52, 55, 62);
pub const TEXT: Color32 = Color32::from_rgb(220, 223, 228);
pub const TEXT_DIM: Color32 = Color32::from_rgb(140, 146, 158);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(92, 97, 108);
pub const ACCENT: Color32 = Color32::from_rgb(0, 120, 212);
pub const ACCENT_HI: Color32 = Color32::from_rgb(56, 152, 236);
pub const LINK: Color32 = Color32::from_rgb(110, 180, 255);
pub const GREEN: Color32 = Color32::from_rgb(78, 190, 105);
pub const RED: Color32 = Color32::from_rgb(232, 85, 85);
pub const ORANGE: Color32 = Color32::from_rgb(240, 160, 60);
pub const YELLOW: Color32 = Color32::from_rgb(235, 200, 90);
pub const PURPLE: Color32 = Color32::from_rgb(190, 145, 255);
/// Value changed by the last step.
pub const CHANGED: Color32 = Color32::from_rgb(255, 196, 80);

// Syntax colours.
pub const SYN_MNEMONIC: Color32 = Color32::from_rgb(86, 156, 214);
pub const SYN_BRANCH: Color32 = Color32::from_rgb(197, 134, 192);
pub const SYN_REGISTER: Color32 = Color32::from_rgb(78, 201, 176);
pub const SYN_NUMBER: Color32 = Color32::from_rgb(181, 206, 168);
pub const SYN_STRING: Color32 = Color32::from_rgb(206, 145, 120);
pub const SYN_COMMENT: Color32 = Color32::from_rgb(106, 153, 85);
pub const SYN_LABEL: Color32 = Color32::from_rgb(220, 220, 170);
pub const SYN_DIRECTIVE: Color32 = Color32::from_rgb(216, 160, 223);
pub const SYN_KEYWORD: Color32 = Color32::from_rgb(100, 140, 200);
pub const SYN_IDENT: Color32 = Color32::from_rgb(200, 200, 160);
pub const SYN_TEXT: Color32 = Color32::from_rgb(212, 212, 212);

pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

pub fn code() -> FontFamily {
    FontFamily::Name("code".into())
}

pub fn code_font(size: f32) -> FontId {
    FontId::new(size, code())
}

pub fn apply_style(ctx: &egui::Context, ui_size: f32) {
    let mut style = egui::Style::default();
    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = CARD;
    v.extreme_bg_color = BG;
    v.faint_bg_color = Color32::from_rgb(32, 33, 38);
    v.code_bg_color = BG;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
    v.selection.bg_fill = Color32::from_rgb(38, 79, 120);
    v.selection.stroke = Stroke::new(1.0, ACCENT_HI);
    v.hyperlink_color = LINK;
    v.override_text_color = None;
    let r = CornerRadius::same(5);
    for (w, fill, stroke) in [
        (&mut v.widgets.noninteractive, PANEL, BORDER),
        (&mut v.widgets.inactive, CARD_HI, Color32::from_rgb(58, 61, 69)),
        (&mut v.widgets.hovered, Color32::from_rgb(54, 57, 66), Color32::from_rgb(80, 84, 95)),
        (&mut v.widgets.active, Color32::from_rgb(60, 64, 74), ACCENT),
        (&mut v.widgets.open, Color32::from_rgb(48, 51, 59), BORDER),
    ] {
        w.corner_radius = r;
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::new(1.0, stroke);
    }
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);

    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(8.0, 6.0);
    s.button_padding = egui::vec2(8.0, 4.0);
    s.menu_margin = egui::Margin::same(6);
    s.interact_size.y = 24.0;
    s.scroll.bar_width = 8.0;
    s.scroll.floating = true;

    use egui::TextStyle::*;
    style.text_styles.insert(Body, FontId::proportional(ui_size));
    style.text_styles.insert(Button, FontId::proportional(ui_size));
    style.text_styles.insert(Small, FontId::proportional(ui_size - 2.5));
    style.text_styles.insert(Heading, FontId::new(ui_size + 4.0, bold()));
    style.text_styles.insert(Monospace, FontId::new(ui_size, code()));
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_style_of(egui::Theme::Dark, style);
}

/// Font folders: the system one and the per-user one.
fn font_dirs() -> Vec<PathBuf> {
    let mut v = vec![std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| r"C:\Windows".into()).join("Fonts")];
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        v.push(PathBuf::from(local).join("Microsoft").join("Windows").join("Fonts"));
    }
    v
}

fn load(file: &str) -> Option<Vec<u8>> {
    font_dirs().into_iter().map(|d| d.join(file)).find(|p| p.exists()).and_then(|p| std::fs::read(p).ok())
}

/// A code font offered in View > Code font.
pub struct CodeFont {
    pub name: &'static str,
    /// Known file names, tried in order.
    files: &'static [&'static str],
    /// If none of those exist: any font file whose name (lower case, without spaces, dashes and
    /// underscores) contains all these words and "regular" (and isn't the proportional "Propo"
    /// variant). Nerd Fonts changed their file names between versions; this finds either.
    words: &'static [&'static str],
}

const fn cf(name: &'static str, files: &'static [&'static str], words: &'static [&'static str]) -> CodeFont {
    CodeFont { name, files, words }
}

/// The code fonts, in menu order. "Built-in" (egui's Hack) always works.
pub const CODE_FONTS: &[CodeFont] = &[
    cf("MesloLGS NF", &["MesloLGS NF Regular.ttf", "MesloLGSNerdFont-Regular.ttf", "MesloLGSNerdFontMono-Regular.ttf"], &["meslolgs"]),
    cf("JetBrainsMono Nerd Font", &["JetBrainsMonoNerdFont-Regular.ttf", "JetBrainsMonoNerdFontMono-Regular.ttf", "JetBrains Mono Regular Nerd Font Complete.ttf", "JetBrains Mono Regular Nerd Font Complete Mono.ttf"], &["jetbrainsmono", "nerd"]),
    cf("FiraCode Nerd Font", &["FiraCodeNerdFont-Regular.ttf", "FiraCodeNerdFontMono-Regular.ttf", "Fira Code Regular Nerd Font Complete.ttf", "Fira Code Regular Nerd Font Complete Mono.ttf"], &["firacode", "nerd"]),
    cf("Hack Nerd Font", &["HackNerdFont-Regular.ttf", "HackNerdFontMono-Regular.ttf", "Hack Regular Nerd Font Complete.ttf", "Hack Regular Nerd Font Complete Mono.ttf"], &["hack", "nerd"]),
    cf("Cascadia Code PL", &["CascadiaCodePL.ttf"], &[]),
    cf("Cascadia Code", &["CascadiaCode.ttf"], &[]),
    cf("Cascadia Mono", &["CascadiaMono.ttf"], &[]),
    cf("Consolas", &["consola.ttf"], &[]),
    cf("JetBrains Mono", &["JetBrainsMono-Regular.ttf", "JetBrainsMono[wght].ttf"], &[]),
    cf("Menlo", &["Menlo-Regular.ttf", "Menlo.ttf"], &[]),
    cf("Courier New", &["cour.ttf"], &[]),
    cf("Lucida Console", &["lucon.ttf"], &[]),
];

/// The built-in fallback (egui's Hack), like CSS "monospace".
pub const BUILTIN_FONT: &str = "Built-in";

fn squash(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, ' ' | '-' | '_')).flat_map(char::to_lowercase).collect()
}

/// A regular-weight .ttf / .otf file whose name contains all `words` (see CodeFont::words).
fn file_matches(file_name: &str, words: &[&str]) -> bool {
    let name = squash(file_name);
    (name.ends_with(".ttf") || name.ends_with(".otf")) && name.contains("regular") && !name.contains("propo") && words.iter().all(|w| name.contains(w))
}

/// The file of `f` on this machine, if it is installed.
fn font_file(f: &CodeFont) -> Option<PathBuf> {
    let dirs = font_dirs();
    if let Some(p) = f.files.iter().flat_map(|n| dirs.iter().map(move |d| d.join(n))).find(|p| p.exists()) {
        return Some(p);
    }
    if f.words.is_empty() {
        return None;
    }
    let mut found: Vec<PathBuf> = dirs
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| file_matches(&n.to_string_lossy(), f.words)))
        .collect();
    found.sort(); // stable choice; the plain variant sorts before "...Mono"
    found.into_iter().next()
}

/// Every code font with whether it is installed (looked up once per run), then "Built-in".
pub fn code_fonts() -> &'static [(&'static str, bool)] {
    static LIST: std::sync::OnceLock<Vec<(&'static str, bool)>> = std::sync::OnceLock::new();
    LIST.get_or_init(|| CODE_FONTS.iter().map(|f| (f.name, font_file(f).is_some())).chain([(BUILTIN_FONT, true)]).collect())
}

pub fn install_fonts(ctx: &egui::Context, code_font: &str) {
    let mut defs = FontDefinitions::default();
    if let Some(regular) = load("segoeui.ttf") {
        defs.font_data.insert("ui".into(), Arc::new(FontData::from_owned(regular)));
        defs.families.entry(FontFamily::Proportional).or_default().insert(0, "ui".into());
    }
    let proportional = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut bold_family = proportional.clone();
    if let Some(b) = load("segoeuisb.ttf").or_else(|| load("segoeuib.ttf")) {
        defs.font_data.insert("ui_bold".into(), Arc::new(FontData::from_owned(b)));
        bold_family.insert(0, "ui_bold".into());
    }
    defs.families.insert(bold(), bold_family);

    let mut code_family = defs.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    // The chosen font; if it isn't installed, the first one that is; "Built-in" = egui's Hack.
    let file = if code_font == BUILTIN_FONT {
        None
    } else {
        CODE_FONTS.iter().find(|f| f.name == code_font).and_then(font_file).or_else(|| CODE_FONTS.iter().find_map(font_file))
    };
    if let Some(bytes) = file.and_then(|p| std::fs::read(p).ok()) {
        defs.font_data.insert("code".into(), Arc::new(FontData::from_owned(bytes)));
        code_family.insert(0, "code".into());
    }
    defs.families.insert(code(), code_family.clone());
    defs.families.insert(FontFamily::Monospace, code_family);

    // Symbol fallbacks so arrows / bullets never render as boxes.
    let mut extra = Vec::new();
    for (key, file) in [("segoe_symbol", "seguisym.ttf"), ("segoe_emoji", "seguiemj.ttf")] {
        if let Some(bytes) = load(file) {
            defs.font_data.insert(key.into(), Arc::new(FontData::from_owned(bytes)));
            extra.push(key.to_string());
        }
    }
    for family in defs.families.values_mut() {
        for k in &extra {
            if !family.contains(k) {
                family.push(k.clone());
            }
        }
    }
    ctx.set_fonts(defs);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nerd_font_file_names_of_both_versions_are_found() {
        let words = &["jetbrainsmono", "nerd"];
        assert!(file_matches("JetBrainsMonoNerdFont-Regular.ttf", words)); // v3
        assert!(file_matches("JetBrains Mono Regular Nerd Font Complete.ttf", words)); // v2
        assert!(!file_matches("JetBrainsMonoNerdFont-Bold.ttf", words)); // not the regular weight
        assert!(!file_matches("JetBrainsMonoNerdFontPropo-Regular.ttf", words)); // proportional
        assert!(!file_matches("JetBrainsMono-Regular.ttf", words)); // not the Nerd Font
        assert!(file_matches("MesloLGS NF Regular.ttf", &["meslolgs"]));
        assert!(!file_matches("HackNerdFont-Regular.woff", &["hack", "nerd"]));
    }

    #[test]
    fn menu_lists_every_font_and_ends_with_built_in() {
        let names: Vec<&str> = code_fonts().iter().map(|f| f.0).collect();
        assert_eq!(names.len(), CODE_FONTS.len() + 1);
        assert_eq!(names[0], "MesloLGS NF");
        assert_eq!(*names.last().unwrap(), BUILTIN_FONT);
        assert!(code_fonts().last().unwrap().1, "Built-in is always available");
    }
}
