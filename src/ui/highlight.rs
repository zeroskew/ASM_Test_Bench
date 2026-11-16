//! Syntax highlighting for the editor (tolerant of half-typed code).

use std::collections::HashMap;

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Stroke};

use super::editor::TAB;
use super::theme::*;
use crate::asm::{self, Reg};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    Text,
    Mnemonic,
    Branch,
    Register,
    Number,
    Str,
    Comment,
    Label,
    Directive,
    Keyword,
    Ident,
    Punct,
}

fn color(c: Class) -> Color32 {
    match c {
        Class::Text => SYN_TEXT,
        Class::Mnemonic => SYN_MNEMONIC,
        Class::Branch => SYN_BRANCH,
        Class::Register => SYN_REGISTER,
        Class::Number => SYN_NUMBER,
        Class::Str => SYN_STRING,
        Class::Comment => SYN_COMMENT,
        Class::Label => SYN_LABEL,
        Class::Directive => SYN_DIRECTIVE,
        Class::Keyword => SYN_KEYWORD,
        Class::Ident => SYN_IDENT,
        Class::Punct => Color32::from_rgb(160, 165, 175),
    }
}

pub const DIRECTIVES: &[&str] = &[
    "section", "segment", "global", "extern", "bits", "db", "dw", "dd", "dq", "resb", "resw", "resd", "resq", "equ", "times", "align", "default", "org",
];
pub const KEYWORDS: &[&str] = &["byte", "word", "dword", "qword", "ptr", "short", "near", "far", "rep", "repe", "repz", "repne", "repnz"];

pub fn is_branch(m: &str) -> bool {
    matches!(m, "jmp" | "call" | "ret" | "loop" | "loope" | "loopz" | "loopne" | "loopnz" | "jecxz" | "enter" | "int" | "int3" | "leave" | "hlt") || (m.starts_with('j') && asm::Cond::parse(&m[1..]).is_some())
}

pub fn classify_word(w: &str) -> Class {
    let l = w.to_ascii_lowercase();
    if Reg::parse(&l).is_some() {
        Class::Register
    } else if DIRECTIVES.contains(&l.as_str()) || l.starts_with('.') && matches!(l.as_str(), ".text" | ".data" | ".bss" | ".rodata") {
        Class::Directive
    } else if KEYWORDS.contains(&l.as_str()) {
        Class::Keyword
    } else if is_branch(&l) {
        Class::Branch
    } else if asm::is_mnemonic(&l) {
        Class::Mnemonic
    } else {
        Class::Ident
    }
}

/// Classes for every char of one line.
pub fn classify_line(line: &str) -> Vec<Class> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = vec![Class::Text; chars.len()];
    let mut i = 0;
    let mut first_word = true;
    while i < chars.len() {
        let c = chars[i];
        if c == ';' {
            for o in &mut out[i..] {
                *o = Class::Comment;
            }
            break;
        }
        if c == '\'' || c == '"' || c == '`' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != c {
                if c == '`' && chars[j] == '\\' {
                    j += 1;
                }
                j += 1;
            }
            let end = (j + 1).min(chars.len());
            for o in &mut out[i..end] {
                *o = Class::Str;
            }
            i = end;
            continue;
        }
        if c.is_ascii_digit() || (c == '$' && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit())) {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                j += 1;
            }
            for o in &mut out[i..j] {
                *o = Class::Number;
            }
            i = j;
            continue;
        }
        if asm_ident_start(c) {
            let mut j = i + 1;
            while j < chars.len() && asm_ident_char(chars[j]) {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            let mut class = classify_word(&word);
            // `label:` at the start of a line, or `name db ...`
            let mut k = j;
            while k < chars.len() && chars[k].is_whitespace() {
                k += 1;
            }
            if first_word && chars.get(j) == Some(&':') {
                class = Class::Label;
                out[j] = Class::Label;
            } else if first_word && class == Class::Ident {
                let rest: String = chars[k..].iter().take_while(|c| asm_ident_char(**c)).collect();
                if DIRECTIVES.contains(&rest.to_ascii_lowercase().as_str()) {
                    class = Class::Label;
                }
            }
            for o in &mut out[i..j] {
                *o = class;
            }
            if class != Class::Label {
                first_word = false;
            }
            i = j;
            continue;
        }
        if !c.is_whitespace() {
            out[i] = Class::Punct;
        }
        i += 1;
    }
    out
}

fn asm_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '.' || c == '?' || c == '@'
}
fn asm_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '?' || c == '@' || c == '$' || c == '#'
}

/// Builds the highlighted layout. `marks`: per line, char ranges to underline in a colour.
/// `italic_comments`: draw comments in italics. `char_w`: width of one column, `tab_w`: how
/// wide epaint draws a tab; tabs are drawn up to the next tab stop.
pub fn layout(text: &str, font: FontId, char_w: f32, tab_w: f32, marks: &HashMap<usize, Vec<(usize, usize, Color32)>>, italic_comments: bool) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    let lines: Vec<&str> = text.split('\n').collect();
    let n = lines.len();
    for (ln, line) in lines.into_iter().enumerate() {
        let classes = classify_line(line);
        let line_marks = marks.get(&ln);
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        // Visual column where the char at `i` starts.
        let mut vcol = 0;
        // epaint draws a tab a fixed width (about TAB columns, rounded to pixels). Each tab ends
        // a section, and the section after it is shifted by the difference, so the text after a
        // tab (and the caret) starts exactly at the next tab stop.
        let mut shift = 0.0;
        while i < chars.len() {
            let cls = classes[i];
            let mark = line_marks.and_then(|m| m.iter().find(|(s, e, _)| i >= *s && i < *e)).map(|m| m.2);
            let leading = shift;
            let mut j = i + 1;
            while j < chars.len() && chars[j - 1] != '\t' && classes[j] == cls && line_marks.and_then(|m| m.iter().find(|(s, e, _)| j >= *s && j < *e)).map(|m| m.2) == mark {
                j += 1;
            }
            let s: String = chars[i..j].iter().collect();
            let mut fmt = TextFormat { font_id: font.clone(), color: color(cls), ..Default::default() };
            if cls == Class::Comment && italic_comments {
                fmt.italics = true;
            }
            if let Some(c) = mark {
                fmt.underline = Stroke::new(1.5, c);
            }
            job.append(&s, leading, fmt);
            let start = vcol;
            vcol = chars[i..j].iter().fold(vcol, |v, &c| if c == '\t' { (v / TAB + 1) * TAB } else { v + 1 });
            // Only the last char of a section can be a tab.
            shift = if chars[j - 1] == '\t' { (TAB - (start + (j - 1 - i)) % TAB) as f32 * char_w - tab_w } else { 0.0 };
            i = j;
        }
        if ln + 1 < n {
            job.append("\n", 0.0, TextFormat { font_id: font.clone(), color: SYN_TEXT, ..Default::default() });
        }
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes() {
        let c = classify_line("loop1: mov eax, [ebx+4] ; hi");
        assert_eq!(c[0], Class::Label);
        assert_eq!(c[7], Class::Mnemonic);
        assert_eq!(c[11], Class::Register);
        assert_eq!(c[17], Class::Register);
        assert_eq!(c[21], Class::Number);
        assert_eq!(c[25], Class::Comment);
        let c = classify_line("msg db 'hi', 10");
        assert_eq!(c[0], Class::Label);
        assert_eq!(c[4], Class::Directive);
        assert_eq!(c[7], Class::Str);
    }

    #[test]
    fn tabs_reach_tab_stops() {
        let ctx = egui::Context::default();
        let font = FontId::monospace(14.0);
        let mut bad = Vec::new();
        let mut out = ctx.run_ui(Default::default(), |ui| {
            let w = ui.fonts_mut(|f| f.glyph_width(&font, '0'));
            let tw = ui.fonts_mut(|f| f.layout_no_wrap("\tx".into(), font.clone(), Color32::WHITE).rows[0].glyphs[1].pos.x);
            for (text, ci, col) in [("\tmov", 1, 4), ("ab\tc", 3, 4), ("abcde\tx\ty", 6, 8), ("abcde\tx\ty", 8, 12), ("\t\tx", 2, 8), ("mov\teax", 4, 4)] {
                let g = ui.fonts_mut(|f| f.layout_job(layout(text, font.clone(), w, tw, &HashMap::new(), false)));
                let x = g.rows[0].glyphs[ci].pos.x;
                // Within pixel snapping.
                if (x - col as f32 * w).abs() > 1.0 {
                    bad.push(format!("{text:?}: char {ci} at {x}, want {}", col as f32 * w));
                }
            }
        });
        out.textures_delta.clear();
        assert!(bad.is_empty(), "{bad:#?}");
    }

    #[test]
    fn italic_comments_option() {
        let font = FontId::monospace(14.0);
        let italic = |on: bool| layout("mov eax, 1 ; hi", font.clone(), 8.0, 32.0, &HashMap::new(), on).sections.iter().any(|s| s.format.italics);
        assert!(italic(true));
        assert!(!italic(false));
    }
}
