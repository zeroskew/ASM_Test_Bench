//! The code editor: highlighted TextEdit plus a gutter with line numbers, breakpoints,
//! the execution arrow and diagnostics markers.

use std::collections::{BTreeSet, HashMap};

use egui::text::{CCursor, CCursorRange};
use egui::{Align, Color32, CornerRadius, Id, Pos2, Rect, Sense, Shape, Stroke, TextBuffer, Ui};

use super::complete::{self, Item, Kind};
use super::{highlight, theme::*};
use crate::asm::{Diagnostic, Severity};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExecKind {
    Paused,
    Fault,
}

pub struct Params<'a> {
    pub font_size: f32,
    pub breakpoints: &'a BTreeSet<usize>,
    /// Lines that hold an instruction (breakpoints elsewhere are drawn hollow).
    pub code_lines: Option<&'a HashMap<usize, u32>>,
    /// Lines that assemble to an instruction in the current text. A new breakpoint on any
    /// other line (comment, label, blank, data) snaps to the next one of these.
    pub bp_lines: Option<&'a BTreeSet<usize>>,
    pub exec_line: Option<(usize, ExecKind)>,
    /// Line of the previously executed instruction.
    pub prev_line: Option<usize>,
    pub diags: &'a [Diagnostic],
    pub scroll_to: Option<usize>,
    /// Execution counts per line, shown after the code.
    pub hits: Option<&'a HashMap<usize, u64>>,
    /// Pad to this column when Space is typed after an instruction (None = off).
    pub operand_column: Option<usize>,
    /// Move a typed `;` after code to this column (None = off).
    pub comment_column: Option<usize>,
    /// Let the cursor go past the end of a line.
    pub virtual_space: bool,
    /// Draw comments in italics.
    pub italic_comments: bool,
    /// Autocomplete candidates (None = autocomplete off).
    pub completions: Option<&'a [Item]>,
}

pub struct Hover {
    pub word: String,
    pub line: usize,
}

pub struct Output {
    pub changed: bool,
    pub toggle_bp: Option<usize>,
    pub run_to: Option<usize>,
    pub cursor: (usize, usize),
    pub hover: Option<Hover>,
    pub response: Option<egui::Response>,
}

pub fn editor_id() -> Id {
    Id::unique("asm_code_editor")
}

fn char_to_byte(s: &str, ci: usize) -> usize {
    s.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(s.len())
}

fn line_col(s: &str, ci: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    for (i, c) in s.chars().enumerate() {
        if i == ci {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Char index of the start of `line`.
fn line_start(s: &str, line: usize) -> usize {
    if line == 0 {
        return 0;
    }
    let mut n = 0;
    for (i, c) in s.chars().enumerate() {
        if c == '\n' {
            n += 1;
            if n == line {
                return i + 1;
            }
        }
    }
    s.chars().count()
}

/// Space between the gutter and the code, in points.
const TEXT_GAP: f32 = 14.0;
/// Gutter: width of the breakpoint column, gap around the arrow, arrow width.
const BP_COL: f32 = 22.0;
const ARROW_GAP: f32 = 7.0;
const ARROW_W: f32 = 9.0;

/// Tab stop width in columns.
pub const TAB: usize = 4;

/// The visual column after `c` drawn at visual column `v` (a tab reaches the next tab stop).
fn advance(v: usize, c: char) -> usize {
    if c == '\t' { (v / TAB + 1) * TAB } else { v + 1 }
}

/// Visual column (tabs expanded to tab stops) of char column `col` in `line`.
pub fn visual_col(line: &str, col: usize) -> usize {
    line.chars().take(col).fold(0, advance)
}

/// Char column in `line` for visual column `vcol` (inside a tab: its nearer edge), and how
/// many visual columns `vcol` lies past the end of the line.
pub fn char_col(line: &str, vcol: usize) -> (usize, usize) {
    let mut v = 0;
    for (i, c) in line.chars().enumerate() {
        let next = advance(v, c);
        if vcol < next {
            return (i + usize::from((vcol - v) * 2 > next - v), 0);
        }
        v = next;
    }
    (line.chars().count(), vcol - v)
}

/// Whitespace that fills visual columns `from..to`: tabs as far as whole tab stops reach, then
/// spaces for the rest.
pub fn gap_fill(from: usize, to: usize) -> String {
    let mut s = String::new();
    let mut v = from;
    while (v / TAB + 1) * TAB <= to {
        s.push('\t');
        v = (v / TAB + 1) * TAB;
    }
    s.push_str(&" ".repeat(to.saturating_sub(v)));
    s
}

/// An instruction mnemonic that takes aligned operands (not a `rep` prefix).
fn is_instruction(w: &str) -> bool {
    let w = w.to_ascii_lowercase();
    crate::asm::is_mnemonic(&w) && !matches!(w.as_str(), "rep" | "repe" | "repz" | "repne" | "repnz")
}

/// Tabs instead of spaces in a line's leading indentation and in the gap between an
/// instruction and its operands (when that gap is an alignment, two or more columns), keeping
/// everything at the same visual column. Whitespace-only lines are left alone.
pub fn tabify_line(l: &str) -> String {
    let n = l.len() - l.trim_start_matches([' ', '\t']).len();
    if n == l.len() {
        return l.to_string();
    }
    let indent_w = visual_col(l, n);
    let mut out = if l[..n].contains(' ') { gap_fill(0, indent_w) } else { l[..n].to_string() };
    let rest = &l[n..];
    let word_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let (word, after) = rest.split_at(word_end);
    let ws = after.len() - after.trim_start_matches([' ', '\t']).len();
    let tail = &after[ws..];
    out.push_str(word);
    if ws > 0 && !tail.is_empty() && !tail.starts_with(';') && is_instruction(word) {
        let from = indent_w + word.chars().count();
        let to = after[..ws].chars().fold(from, advance);
        if to - from >= 2 || after[..ws].contains('\t') {
            out.push_str(&gap_fill(from, to));
            out.push_str(tail);
            return out;
        }
    }
    out.push_str(after);
    out
}

/// [`tabify_line`] for every line of `text`.
pub fn tabify(text: &str) -> String {
    text.split('\n').map(tabify_line).collect::<Vec<_>>().join("\n")
}

/// Where caret (line, col) of `old` lands in `new`, the same text re-indented (same visual
/// column).
pub fn map_caret(old: &str, new: &str, (line, col): (usize, usize)) -> (usize, usize) {
    let v = visual_col_at(old, line, col);
    (line, char_col(new.split('\n').nth(line).unwrap_or(""), v).0)
}

/// Visual column of char column `col` on line `line` of `text`.
pub fn visual_col_at(text: &str, line: usize, col: usize) -> usize {
    visual_col(text.split('\n').nth(line).unwrap_or(""), col)
}

// ---- Virtual space ----
// The cursor may sit past the end of a line. That is done by padding the line with spaces
// and remembering (line, original length); if the user moves away without typing there, the
// padding is removed again.

fn vpad_id() -> Id {
    editor_id().with("virtual_pad")
}

fn get_vpad(ctx: &egui::Context) -> Option<(usize, usize)> {
    ctx.data(|d| d.get_temp::<(usize, usize)>(vpad_id()))
}

fn set_vpad(ctx: &egui::Context, v: Option<(usize, usize)>) {
    ctx.data_mut(|d| {
        match v {
            Some(v) => {
                d.insert_temp(vpad_id(), v);
            }
            None => {
                d.remove::<(usize, usize)>(vpad_id());
            }
        };
    });
}

fn line_len(text: &str, line: usize) -> usize {
    text.split('\n').nth(line).map(|l| l.chars().count()).unwrap_or(0)
}

/// Visual width of `line` (tabs expanded).
fn line_vlen(text: &str, line: usize) -> usize {
    let l = text.split('\n').nth(line).unwrap_or("");
    visual_col(l, l.chars().count())
}

/// Pads `line` with spaces up to visual column `vcol`. Returns how many were added.
pub fn pad_line(text: &mut String, line: usize, vcol: usize) -> usize {
    let (len, vlen) = (line_len(text, line), line_vlen(text, line));
    if vcol <= vlen {
        return 0;
    }
    let at = char_to_byte(text, line_start(text, line) + len);
    text.insert_str(at, &" ".repeat(vcol - vlen));
    vcol - vlen
}

/// Char index on `line` for visual column `vcol` (clamped to the end of the line).
fn ci_at_vcol(text: &str, line: usize, vcol: usize) -> usize {
    line_start(text, line) + char_col(text.split('\n').nth(line).unwrap_or(""), vcol).0
}

/// Removes the virtual padding of `line` (everything after char column `start`) if it is
/// still only spaces. Returns how many chars were removed.
pub fn trim_pad(text: &mut String, line: usize, start: usize) -> usize {
    let len = line_len(text, line);
    if len <= start {
        return 0;
    }
    let ls = line_start(text, line);
    if !text.chars().skip(ls + start).take(len - start).all(|c| c == ' ') {
        return 0;
    }
    let (a, b) = (char_to_byte(text, ls + start), char_to_byte(text, ls + len));
    text.replace_range(a..b, "");
    len - start
}

/// Records padding on `line` that started at `start`, keeping the original start if that
/// line already had padding.
fn note_vpad(ctx: &egui::Context, line: usize, start: usize) {
    let start = match get_vpad(ctx) {
        Some((l, s)) if l == line => s.min(start),
        _ => start,
    };
    set_vpad(ctx, Some((line, start)));
}

/// Where the text was drawn last frame: (galley top-left, visible text rect), for hit-testing
/// a click before the TextEdit runs.
fn geom_id() -> Id {
    editor_id().with("geom")
}

/// The view was just scrolled (to the caret, a page, the execution line). This pass was drawn
/// with the old scroll position: the caret line at the edge, the text still where it was. Drop
/// it and draw again right away, so what is shown never jumps there and back.
fn redraw_scrolled(ctx: &egui::Context) {
    ctx.request_discard("editor scrolled");
}

fn caret_hold_id() -> Id {
    editor_id().with("caret_hold")
}

/// How long the caret stays solid after it moves or the text scrolls.
const CARET_HOLD_SECS: f64 = 0.6;

/// Keeps the caret from blinking for a moment (it moved, or the text scrolled).
fn hold_caret(ctx: &egui::Context) {
    let until = ctx.input(|i| i.time) + CARET_HOLD_SECS;
    ctx.data_mut(|d| d.insert_temp(caret_hold_id(), until));
    ctx.request_repaint();
}

/// Before the TextEdit runs: a mouse press past the end of a line pads that line up to the
/// pointer, so the TextEdit itself puts the caret there on press (and keeps it there while the
/// button is held) instead of at the end of the line.
fn virtual_space_press(ui: &Ui, text: &mut String, row_h: f32, char_w: f32) {
    let ctx = ui.ctx();
    let pressed = ctx.input(|i| i.pointer.primary_pressed() && !i.modifiers.shift);
    let Some(pos) = pressed.then(|| ctx.input(|i| i.pointer.interact_pos())).flatten() else { return };
    let Some((gpos, rect)) = ctx.data(|d| d.get_temp::<(Pos2, Rect)>(geom_id())) else { return };
    if !rect.contains(pos) || ctx.layer_id_at(pos) != Some(ui.layer_id()) {
        return;
    }
    let n_lines = text.split('\n').count();
    let l = (((pos.y - gpos.y) / row_h).floor().max(0.0) as usize).min(n_lines - 1);
    let col = ((pos.x - gpos.x) / char_w).round().max(0.0) as usize;
    if col <= line_vlen(text, l) {
        return;
    }
    if let Some((pl, start)) = get_vpad(ctx) {
        if pl != l {
            trim_pad(text, pl, start);
            set_vpad(ctx, None);
        }
    }
    let len = line_len(text, l);
    pad_line(text, l, col);
    note_vpad(ctx, l, len);
}

/// Before the TextEdit runs: drops padding the cursor has left, and handles Up / Down
/// (keep the column) and Right at the end of a line (move into virtual space).
fn virtual_space_keys(ui: &mut Ui, text: &mut String, focused: bool) {
    let ctx = ui.ctx().clone();
    let id = editor_id();
    let Some(mut state) = egui::TextEdit::load_state(&ctx, id) else { return };
    let Some(range) = state.cursor.char_range() else { return };
    let mut ci = range.primary.index.0;
    let collapsed = range.primary.index == range.secondary.index;
    let (mut line, col) = line_col(text, ci);
    let vcol = visual_col_at(text, line, col);

    // Padding on a line the cursor has left: remove it (unless the user typed there).
    if let Some((pl, start)) = get_vpad(&ctx) {
        if pl != line {
            let removed = trim_pad(text, pl, start);
            set_vpad(&ctx, None);
            if removed > 0 && pl < line {
                ci -= removed;
                let shift = |c: CCursor| if c.index.0 > line_start(text, pl) + start { CCursor::new(c.index.0 - removed) } else { c };
                state.cursor.set_char_range(Some(CCursorRange::two(shift(range.secondary), CCursor::new(ci))));
                state.clone().store(&ctx, id);
            }
            line = line_col(text, ci).0;
        } else if line_len(text, pl) > start && !text.split('\n').nth(pl).unwrap_or("").chars().skip(start).all(|c| c == ' ') {
            set_vpad(&ctx, None); // typed something: the spaces are real now
        }
    }

    if !focused || !collapsed {
        return;
    }
    let plain = ui.input(|i| i.modifiers.is_none());
    if !plain {
        return;
    }
    let n_lines = text.split('\n').count();
    let pressed = |ui: &mut Ui, k: egui::Key| ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, k));
    let target = if pressed(ui, egui::Key::ArrowUp) {
        Some(line.saturating_sub(1))
    } else if pressed(ui, egui::Key::ArrowDown) {
        Some((line + 1).min(n_lines - 1))
    } else {
        None
    };
    let mut new_ci = None;
    if let Some(t) = target {
        if t != line {
            // Leaving this line: drop its padding first.
            if let Some((pl, start)) = get_vpad(&ctx) {
                if pl == line {
                    trim_pad(text, pl, start);
                    set_vpad(&ctx, None);
                }
            }
            let len = line_len(text, t);
            if pad_line(text, t, vcol) > 0 {
                note_vpad(&ctx, t, len);
            }
            new_ci = Some(ci_at_vcol(text, t, vcol));
        } else {
            new_ci = Some(ci);
        }
    } else if line_len(text, line) == col && pressed(ui, egui::Key::ArrowRight) {
        pad_line(text, line, vcol + 1);
        note_vpad(&ctx, line, col);
        new_ci = Some(ci + 1);
    }
    if let Some(c) = new_ci {
        state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(c))));
        state.store(&ctx, id);
    }
}

/// Before the TextEdit (which doesn't handle these keys): PageUp / PageDown move the caret a
/// screenful up / down in the same column (Shift: extend the selection). Returns the line the
/// caret was on, so the text can then scroll by exactly as much as the caret moved and the
/// caret stays at the same height on screen.
fn page_keys(ui: &mut Ui, text: &mut String, focused: bool, virtual_space: bool, row_h: f32) -> Option<usize> {
    if !focused {
        return None;
    }
    // (up?, with Shift?) of the key press, if any.
    let pressed = ui.input_mut(|i| {
        let (shift, none) = (egui::Modifiers::SHIFT, egui::Modifiers::NONE);
        [(egui::Key::PageUp, true), (egui::Key::PageDown, false)].into_iter().find_map(|(k, up)| {
            if i.consume_key(shift, k) {
                Some((up, true))
            } else if i.consume_key(none, k) {
                Some((up, false))
            } else {
                None
            }
        })
    });
    let (up, extend) = pressed?;
    let ctx = ui.ctx().clone();
    let id = editor_id();
    let mut state = egui::TextEdit::load_state(&ctx, id)?;
    let range = state.cursor.char_range()?;
    let (line, col) = line_col(text, range.primary.index.0);
    let n_lines = text.split('\n').count();
    // How many lines fit in the view (as it was drawn last frame).
    let view_h = ctx.data(|d| d.get_temp::<f32>(editor_id().with("view_h")))?;
    let rows = ((view_h / row_h).floor() as usize).max(2);
    let page = rows - 1;
    let target = if up { line.saturating_sub(page) } else { (line + page).min(n_lines - 1) };

    // Leaving this line: drop its virtual-space padding; in the new line keep the column.
    if let Some((pl, start)) = get_vpad(&ctx) {
        if pl == line && target != line {
            trim_pad(text, pl, start);
            set_vpad(&ctx, None);
        }
    }
    // Keep the visual column (tabs expanded), not the char column.
    let vcol = visual_col_at(text, line, col);
    let len = line_len(text, target);
    if virtual_space && !extend && pad_line(text, target, vcol) > 0 {
        note_vpad(&ctx, target, len);
    }
    let ci = ci_at_vcol(text, target, vcol);
    let new_range = if extend { CCursorRange::two(range.secondary, CCursor::new(ci)) } else { CCursorRange::one(CCursor::new(ci)) };
    state.cursor.set_char_range(Some(new_range));
    state.store(&ctx, id);
    Some(line)
}

/// True if a `;` at char column `at` would start a comment: not inside a string and not
/// after an earlier comment `;`.
fn starts_comment(chars: &[char], at: usize) -> bool {
    let mut quote: Option<char> = None;
    for &c in &chars[..at] {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '\'' || c == '"' || c == '`' => quote = Some(c),
            None if c == ';' => return false,
            None => {}
        }
    }
    quote.is_none()
}

/// For a `;` just typed at char column `at` of `line` (cursor right after it, nothing else
/// after it on the line), when it starts a comment: (spaces to insert before the `;`, add a
/// space after it). After code, the `;` moves to `col`, or gets one separating space if the
/// code already reaches `col`; on a comment-only line it stays put. Either way the comment
/// text starts after `"; "`.
pub fn comment_edit(line: &str, at: usize, col: usize) -> Option<(usize, bool)> {
    let chars: Vec<char> = line.chars().collect();
    if chars.get(at) != Some(&';') || chars[at + 1..].iter().any(|c| !c.is_whitespace()) || !starts_comment(&chars, at) {
        return None;
    }
    let before = &chars[..at];
    let vat = visual_col(line, at);
    let pad = if before.iter().all(|c| c.is_whitespace()) {
        0 // a comment-only line stays where it is
    } else if vat < col {
        col - vat
    } else if !before.last().is_some_and(|c| c.is_whitespace()) {
        1
    } else {
        0
    };
    Some((pad, true))
}

/// A space typed at char column `at` right after an auto-added `"; "` is dropped, so typing
/// `; ` by habit still gives a single space.
pub fn swallow_comment_space(line: &str, at: usize) -> bool {
    let chars: Vec<char> = line.chars().collect();
    at >= 2 && chars.get(at) == Some(&' ') && chars[at - 1] == ' ' && chars[at - 2] == ';' && chars[at + 1..].iter().all(|c| c.is_whitespace()) && starts_comment(&chars, at - 2)
}

/// Extra spaces to add after a space typed at char column `at` of `line`, so the operands
/// start at `col`. Only when the text before it is `[label:] mnemonic` (an instruction, not a
/// rep prefix or a directive) and nothing follows on the line. If the mnemonic already reaches
/// `col`, pads to the next tab stop instead.
pub fn operand_padding(line: &str, at: usize, col: usize) -> Option<usize> {
    let chars: Vec<char> = line.chars().collect();
    if chars.get(at) != Some(&' ') || chars[at + 1..].iter().any(|c| !c.is_whitespace()) {
        return None;
    }
    let before: String = chars[..at].iter().collect();
    let code = match before.split_once(':') {
        Some((label, rest)) if !label.trim().is_empty() && !label.contains(char::is_whitespace) => rest,
        _ => &before,
    };
    let word = code.trim_start();
    if word.is_empty() || word.contains(char::is_whitespace) || code.ends_with(' ') {
        return None;
    }
    let w = word.to_ascii_lowercase();
    if !crate::asm::is_mnemonic(&w) || matches!(w.as_str(), "rep" | "repe" | "repz" | "repne" | "repnz") {
        return None;
    }
    let vat = visual_col(line, at);
    let target = if vat < col { col } else { (vat / TAB + 1) * TAB };
    // The typed space already covers one column.
    Some(target - vat - 1).filter(|n| *n > 0)
}

/// Indents (adds a tab) or unindents (removes one leading tab, or up to `TAB` leading spaces)
/// lines `a..=b`. Returns the new char range covering those lines.
pub fn indent_lines(text: &mut String, a: usize, b: usize, indent: bool) -> (usize, usize) {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let b = b.min(lines.len().saturating_sub(1));
    for l in &mut lines[a..=b] {
        if indent {
            if !l.trim().is_empty() {
                l.insert(0, '\t');
            }
        } else {
            l.drain(..unindent_len(l));
        }
    }
    *text = lines.join("\n");
    let start = line_start(text, a);
    let end = line_start(text, b) + text.split('\n').nth(b).map(|l| l.chars().count()).unwrap_or(0);
    (start, end)
}

/// Bytes one unindent step removes from the start of `l`: up to one tab stop of leading
/// whitespace (spaces, ending early at a tab, which is removed too).
fn unindent_len(l: &str) -> usize {
    let mut n = 0;
    for c in l.chars().take(TAB) {
        match c {
            ' ' => n += 1,
            '\t' => return n + 1,
            _ => break,
        }
    }
    n
}

/// Toggles `; ` comments on lines `a..=b`. Returns the new char range covering those lines.
pub fn toggle_comment(text: &mut String, a: usize, b: usize) -> (usize, usize) {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let b = b.min(lines.len().saturating_sub(1));
    let all_commented = lines[a..=b].iter().filter(|l| !l.trim().is_empty()).all(|l| l.trim_start().starts_with(';'));
    let indent = lines[a..=b].iter().filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
    for l in &mut lines[a..=b] {
        if l.trim().is_empty() {
            continue;
        }
        if all_commented {
            let pos = l.len() - l.trim_start().len();
            l.remove(pos);
            if l[pos..].starts_with(' ') {
                l.remove(pos);
            }
        } else {
            l.insert_str(indent, "; ");
        }
    }
    *text = lines.join("\n");
    let start = line_start(text, a);
    let end = line_start(text, b) + text.split('\n').nth(b).map(|l| l.chars().count()).unwrap_or(0);
    (start, end)
}

// ---- Tabs ----
// One editor widget shows whichever document is active; switching tabs moves its state
// (caret, selection, undo history) out and the other document's in.

/// Before the editor shows another document: removes virtual-space padding from `text`,
/// closes the autocomplete popup, and returns the editor state to keep with this document.
pub fn detach(ctx: &egui::Context, text: &mut String) -> Option<egui::text_edit::TextEditState> {
    if let Some((line, start)) = get_vpad(ctx) {
        trim_pad(text, line, start);
        set_vpad(ctx, None);
    }
    close_popup(ctx);
    let state = egui::TextEdit::load_state(ctx, editor_id());
    ctx.data_mut(|d| {
        d.remove::<(usize, usize)>(editor_id().with("last_caret"));
        d.remove::<egui::text_edit::TextEditState>(editor_id());
    });
    state
}

/// The editor now shows a document again: restores its state (None: a fresh document) and
/// gives the editor the keyboard focus.
pub fn attach(ctx: &egui::Context, state: Option<egui::text_edit::TextEditState>) {
    if let Some(st) = state {
        st.store(ctx, editor_id());
    }
    ctx.memory_mut(|m| m.request_focus(editor_id()));
}

// ---- Autocomplete ----
// While a word is typed, a popup below it lists what can go there (instructions at the start
// of a statement, registers / variables / labels in operands, ...) with a short description.
// Up / Down choose, Enter or Tab inserts, Esc closes, Ctrl+Space opens it on demand.

/// Rows shown at once in the popup.
const AC_ROWS: usize = 8;
const AC_WIDTH: f32 = 340.0;

#[derive(Clone, Default)]
struct Popup {
    open: bool,
    /// Line and char column of the word being completed.
    line: usize,
    start: usize,
    selected: usize,
    /// First visible row (the list scrolls with the selection).
    first: usize,
    /// Opened with Ctrl+Space: may show everything for an empty word.
    forced: bool,
}

fn popup_id() -> Id {
    editor_id().with("autocomplete")
}

fn get_popup(ctx: &egui::Context) -> Popup {
    ctx.data(|d| d.get_temp::<Popup>(popup_id())).unwrap_or_default()
}

fn set_popup(ctx: &egui::Context, p: Popup) {
    ctx.data_mut(|d| d.insert_temp(popup_id(), p));
}

fn close_popup(ctx: &egui::Context) {
    set_popup(ctx, Popup::default());
}

/// The caret's char index when nothing is selected.
fn collapsed_caret(ctx: &egui::Context) -> Option<usize> {
    let r = egui::TextEdit::load_state(ctx, editor_id())?.cursor.char_range()?;
    (r.primary.index == r.secondary.index).then_some(r.primary.index.0)
}

/// The word at the caret and what completes it, if the popup is open on that word.
fn popup_matches<'a>(text: &str, ci: usize, pop: &Popup, items: &'a [Item]) -> Option<(complete::Ctx, Vec<&'a Item>)> {
    let (line, col) = line_col(text, ci);
    if !pop.open || line != pop.line {
        return None;
    }
    let ctx = complete::context(text.split('\n').nth(line)?, col, pop.forced)?;
    if ctx.start != pop.start {
        return None;
    }
    let m = complete::matches(items, &ctx);
    (!m.is_empty()).then_some((ctx, m))
}

/// Replaces the word with `item`; returns the new caret char index.
fn accept_completion(ctx: &egui::Context, text: &mut String, line: usize, c: &complete::Ctx, item: &Item) -> usize {
    let ls = line_start(text, line);
    let (a, b) = (char_to_byte(text, ls + c.start), char_to_byte(text, ls + c.end));
    let ins = complete::insert_text(item, &c.prefix);
    text.replace_range(a..b, &ins);
    let ci = ls + c.start + ins.chars().count();
    if let Some(mut st) = egui::TextEdit::load_state(ctx, editor_id()) {
        st.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(ci))));
        st.store(ctx, editor_id());
    }
    close_popup(ctx);
    ci
}

/// Before the TextEdit (and the Tab / arrow handling): Ctrl+Space opens the popup; while it
/// is open, Up / Down / PageUp / PageDown choose, Enter / Tab insert, Esc closes. Closes the
/// popup when the caret has left the word or nothing matches any more.
fn autocomplete_keys(ui: &mut Ui, text: &mut String, items: &[Item], focused: bool, result: &mut Output) {
    let ctx = ui.ctx().clone();
    if focused && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Space)) {
        if let Some(ci) = collapsed_caret(&ctx) {
            let (line, col) = line_col(text, ci);
            if let Some(c) = text.split('\n').nth(line).and_then(|l| complete::context(l, col, true)) {
                set_popup(&ctx, Popup { open: true, line, start: c.start, forced: true, ..Default::default() });
            }
        }
    }
    let mut pop = get_popup(&ctx);
    if !pop.open {
        return;
    }
    let found = if focused { collapsed_caret(&ctx).and_then(|ci| popup_matches(text, ci, &pop, items)) } else { None };
    let Some((c, m)) = found else {
        close_popup(&ctx);
        return;
    };
    let n = m.len();
    pop.selected = pop.selected.min(n - 1);
    let key = |ui: &mut Ui, k: egui::Key| ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, k));
    if key(ui, egui::Key::Escape) {
        close_popup(&ctx);
        return;
    }
    if key(ui, egui::Key::Enter) || key(ui, egui::Key::Tab) {
        accept_completion(&ctx, text, pop.line, &c, m[pop.selected]);
        result.changed = true;
        return;
    }
    if key(ui, egui::Key::ArrowDown) {
        pop.selected = (pop.selected + 1) % n;
    }
    if key(ui, egui::Key::ArrowUp) {
        pop.selected = (pop.selected + n - 1) % n;
    }
    if key(ui, egui::Key::PageDown) {
        pop.selected = (pop.selected + AC_ROWS).min(n - 1);
    }
    if key(ui, egui::Key::PageUp) {
        pop.selected = pop.selected.saturating_sub(AC_ROWS);
    }
    set_popup(&ctx, pop);
}

/// After a character was typed: open (or refresh) the popup for the word at the caret.
fn autocomplete_after_typing(ctx: &egui::Context, text: &str, ci: usize, typed: Option<char>, items: &[Item]) {
    let word_char = typed.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    if !word_char {
        if typed.is_some() {
            close_popup(ctx); // space, comma, bracket, ...: the word is done
        }
        return;
    }
    let (line, col) = line_col(text, ci);
    let Some(c) = text.split('\n').nth(line).and_then(|l| complete::context(l, col, false)) else {
        close_popup(ctx);
        return;
    };
    if complete::matches(items, &c).is_empty() {
        close_popup(ctx);
        return;
    }
    let pop = get_popup(ctx);
    let same_word = pop.open && pop.line == line && pop.start == c.start;
    set_popup(ctx, Popup { open: true, line, start: c.start, forced: same_word && pop.forced, ..Default::default() });
}

fn kind_color(k: Kind) -> Color32 {
    match k {
        Kind::Instruction => SYN_MNEMONIC,
        Kind::Register => SYN_REGISTER,
        Kind::Label => SYN_LABEL,
        Kind::Variable => SYN_IDENT,
        Kind::Constant => SYN_NUMBER,
        Kind::Directive | Kind::Section => SYN_DIRECTIVE,
        Kind::Keyword => SYN_KEYWORD,
    }
}

/// Draws the popup under (or above) the word; returns the index of a clicked row.
fn draw_popup(ui: &Ui, word_rect: Rect, m: &[&Item], pop: &mut Popup, code_size: f32) -> Option<usize> {
    let n = m.len();
    if pop.selected < pop.first {
        pop.first = pop.selected;
    } else if pop.selected >= pop.first + AC_ROWS {
        pop.first = pop.selected + 1 - AC_ROWS;
    }
    pop.first = pop.first.min(n.saturating_sub(AC_ROWS));
    let name_font = code_font((code_size - 1.0).max(11.0));
    let small = egui::FontId::proportional(12.0);
    let row_h = ui.fonts_mut(|f| f.row_height(&name_font)) + 6.0;
    // Below the word if it fits, else above it.
    let screen = ui.ctx().content_rect();
    let est_h = row_h * n.min(AC_ROWS) as f32 + 90.0;
    let below = word_rect.bottom() + 2.0 + est_h < screen.bottom();
    let (pos, pivot) = if below { (word_rect.left_bottom() + egui::vec2(-6.0, 2.0), egui::Align2::LEFT_TOP) } else { (word_rect.left_top() + egui::vec2(-6.0, -2.0), egui::Align2::LEFT_BOTTOM) };
    let mut clicked = None;
    egui::Area::new(popup_id().with("area")).order(egui::Order::Foreground).fixed_pos(pos).pivot(pivot).show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
            ui.set_width(AC_WIDTH);
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            for i in pop.first..(pop.first + AC_ROWS).min(n) {
                let it = m[i];
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(AC_WIDTH, row_h), Sense::click());
                let p = ui.painter();
                if i == pop.selected {
                    p.rect_filled(rect, 4.0, Color32::from_rgb(38, 79, 120));
                } else if resp.hovered() {
                    p.rect_filled(rect, 4.0, Color32::from_rgb(48, 51, 59));
                }
                if resp.clicked() {
                    clicked = Some(i);
                }
                let mid = rect.center().y;
                p.text(Pos2::new(rect.left() + 8.0, mid), egui::Align2::LEFT_CENTER, &it.name, name_font.clone(), kind_color(it.kind));
                p.text(Pos2::new(rect.right() - 8.0, mid), egui::Align2::RIGHT_CENTER, it.kind.tag(), small.clone(), TEXT_FAINT);
            }
            if n > AC_ROWS {
                ui.add_space(2.0);
                ui.label(egui::RichText::new(format!("{} of {n}  ·  ↑↓ to choose", pop.selected + 1)).size(11.0).color(TEXT_FAINT));
            }
            // Help for the selected item: its syntax / value, then what it does.
            let it = m[pop.selected];
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);
            egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 2)).show(ui, |ui| {
                ui.set_width(AC_WIDTH - 16.0);
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 3.0);
                ui.label(egui::RichText::new(&it.detail).font(name_font.clone()).color(kind_color(it.kind)));
                ui.label(egui::RichText::new(&it.doc).size(12.5).color(TEXT));
                ui.label(egui::RichText::new("Enter / Tab insert  ·  Esc close").size(11.0).color(TEXT_FAINT));
            });
        });
    });
    clicked
}


/// The line a breakpoint toggle at `line` acts on: the line itself if it already has a
/// breakpoint (so stray ones can always be removed) or holds an instruction, otherwise the
/// next instruction line. None when there is no instruction at or below `line`, or when that
/// next line already has a breakpoint (clicking a comment shouldn't remove one below it).
pub fn bp_target(bps: &BTreeSet<usize>, instr_lines: Option<&BTreeSet<usize>>, line: usize) -> Option<usize> {
    if bps.contains(&line) {
        return Some(line);
    }
    let Some(lines) = instr_lines else { return Some(line) };
    let next = lines.range(line..).next().copied()?;
    (next == line || !bps.contains(&next)).then_some(next)
}

pub fn show(ui: &mut Ui, text: &mut String, p: &Params) -> Output {
    let font = code_font(p.font_size);
    let (row_h, char_w) = ui.fonts_mut(|f| (f.row_height(&font), f.glyph_width(&font, '0')));
    // How wide epaint draws a tab (a fixed width, rounded to pixels).
    let tab_w = ui.fonts_mut(|f| f.layout_no_wrap("\tx".into(), font.clone(), Color32::WHITE).rows[0].glyphs.get(1).map_or(TAB as f32 * char_w, |g| g.pos.x));
    // Vertical middle of a digit within a row, so the arrow and breakpoint dot line up
    // with the line numbers (the row also holds space below the baseline).
    let digit_mid = ui.fonts_mut(|f| {
        let g = f.layout_no_wrap("0".into(), font.clone(), Color32::WHITE);
        g.rows.first().and_then(|r| r.glyphs.first()).map(|gl| gl.pos.y + gl.uv_rect.offset.y + gl.uv_rect.size.y * 0.5).filter(|y| *y > 0.0)
    });
    let n_lines = text.split('\n').count();
    let digits = n_lines.to_string().len().max(3) as f32;
    // Gutter columns: [breakpoint dot][line number][gap][arrow][gap][diagnostic bar]
    let num_right = BP_COL + digits * char_w;
    let arrow_left = num_right + ARROW_GAP;
    let gutter_w = arrow_left + ARROW_W + ARROW_GAP + 2.0;
    let id = editor_id();
    let mut result = Output { changed: false, toggle_bp: None, run_to: None, cursor: (0, 0), hover: None, response: None };

    // Underlines for diagnostics.
    let mut marks: HashMap<usize, Vec<(usize, usize, Color32)>> = HashMap::new();
    for d in p.diags {
        if d.cols.1 > d.cols.0 {
            let c = if d.severity == Severity::Error { RED } else { YELLOW };
            marks.entry(d.line).or_default().push((d.cols.0, d.cols.1, c));
        }
    }
    let mut layouter = |ui: &Ui, buf: &dyn TextBuffer, _w: f32| {
        let job = highlight::layout(buf.as_str(), font.clone(), char_w, tab_w, &marks, p.italic_comments);
        ui.fonts_mut(|f| f.layout_job(job))
    };

    let prev_len = text.chars().count();
    // Ctrl+/ toggles comments (handled before the TextEdit so it doesn't see the key).
    let focused = ui.memory(|m| m.has_focus(id));
    // Autocomplete popup keys first, so Up / Down / Tab / Enter go to the popup while it's open.
    if let Some(items) = p.completions {
        autocomplete_keys(ui, text, items, focused, &mut result);
    } else {
        close_popup(ui.ctx());
    }
    let toggle = focused && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Slash));
    if toggle {
        if let Some(state) = egui::TextEdit::load_state(ui.ctx(), id) {
            if let Some(r) = state.cursor.char_range() {
                let [a, b] = r.sorted_cursors();
                let la = line_col(text, a.index.0).0;
                let mut lb = line_col(text, b.index.0).0;
                if lb > la && line_col(text, b.index.0).1 == 0 {
                    lb -= 1;
                }
                let (s, e) = toggle_comment(text, la, lb);
                let mut st = state.clone();
                st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(s), CCursor::new(e))));
                st.store(ui.ctx(), id);
                result.changed = true;
            }
        }
    }

    // Tab / Shift+Tab with real tab stops (egui's own Tab inserts a '\t' and only
    // unindents one line). Shift+Tab first: consume_key ignores extra Shift.
    let unindent = focused && ui.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab));
    let tab = !unindent && focused && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
    if unindent || tab {
        if let Some(state) = egui::TextEdit::load_state(ui.ctx(), id) {
            if let Some(r) = state.cursor.char_range() {
                let [a, b] = r.sorted_cursors();
                let (a, b) = (a.index.0, b.index.0);
                let (la, ca) = line_col(text, a);
                let (mut lb, cb) = line_col(text, b);
                if lb > la && cb == 0 {
                    lb -= 1;
                }
                let new_range = if tab && la == lb {
                    // A tab character (drawn up to the next tab stop), replacing any selection.
                    let (start, end) = (char_to_byte(text, a), char_to_byte(text, b));
                    text.replace_range(start..end, "\t");
                    (a + 1, a + 1)
                } else {
                    let (s, e) = indent_lines(text, la, lb, tab);
                    if la == lb {
                        // Unindent of a single line: keep the caret on the same character.
                        let removed = (prev_len - text.chars().count()).min(ca);
                        (a - removed, a - removed)
                    } else {
                        (s, e)
                    }
                };
                let mut st = state.clone();
                st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(new_range.0), CCursor::new(new_range.1))));
                st.store(ui.ctx(), id);
                result.changed = true;
            }
        }
    }
    if p.virtual_space {
        virtual_space_keys(ui, text, focused);
        virtual_space_press(ui, text, row_h, char_w);
    }
    let page_from = page_keys(ui, text, focused, p.virtual_space, row_h);
    let prev_len = text.chars().count();

    // Scrolling to the caret / execution line is instant, like code editors: an animation
    // restarted ~30 times a second (holding an arrow key) makes the text lurch. Mouse-wheel
    // scrolling stays smooth (that smoothing happens in the input, not here).
    egui::ScrollArea::both().id_salt("editor_scroll").auto_shrink([false, false]).animated(false).show(ui, |ui| {
        let avail = ui.available_size();
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            let height = (n_lines as f32 * row_h + 16.0).max(avail.y);
            let (gutter_rect, _) = ui.allocate_exact_size(egui::vec2(gutter_w, height), Sense::hover());
            // Breathing room between the gutter and the code (the TextEdit margin doesn't
            // apply without a frame).
            ui.add_space(TEXT_GAP);
            let bg_idx = ui.painter().add(Shape::Noop);
            // While the caret moves or the text scrolls, the caret stays solid (no blinking);
            // it starts blinking again shortly after things settle.
            // A key press or wheel scroll this frame counts already, so the caret isn't caught
            // in its "off" phase on the very frame it moves.
            let input_now = ui.input(|i| i.smooth_scroll_delta != egui::Vec2::ZERO || i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. })));
            if input_now && ui.memory(|m| m.has_focus(id)) {
                hold_caret(ui.ctx());
            }
            let hold_until = ui.ctx().data(|d| d.get_temp::<f64>(caret_hold_id())).unwrap_or(0.0);
            let now = ui.input(|i| i.time);
            if now < hold_until {
                ui.visuals_mut().text_cursor.blink = false;
                ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(hold_until - now));
            }
            let te = egui::TextEdit::multiline(text)
                .id(id)
                .font(font.clone())
                .lock_focus(true)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin { left: 10, right: 40, top: 6, bottom: 10 })
                .desired_width(f32::INFINITY)
                .min_size(egui::vec2((avail.x - gutter_w - TEXT_GAP).max(100.0), avail.y))
                .layouter(&mut layouter);
            let out = te.show(ui);
            let resp = out.response.response.clone();
            let galley = out.galley.clone();
            let gpos = out.galley_pos;

            // Row rectangles in screen space, one per line (no wrapping).
            let row_rect = |line: usize| -> Rect {
                let y = match galley.rows.get(line) {
                    Some(r) if galley.rows.len() == n_lines => gpos.y + r.pos.y,
                    _ => gpos.y + line as f32 * row_h,
                };
                Rect::from_min_max(Pos2::new(gutter_rect.left(), y), Pos2::new(ui.clip_rect().right().max(gutter_rect.right() + 2000.0), y + row_h))
            };

            // Cursor position.
            let cursor_ci = out.cursor_range.map(|r| r.primary.index.0);
            if let Some(ci) = cursor_ci {
                result.cursor = line_col(text, ci);
            }

            // Text change post-processing: auto-indent (tabs are kept as typed / pasted).
            if resp.changed() {
                result.changed = true;
                let mut ci = cursor_ci.unwrap_or(0);
                let mut fixed = false;
                let now_len = text.chars().count();
                if now_len == prev_len + 1 && ci > 0 && text.chars().nth(ci - 1) == Some('\n') {
                    let (line, _) = line_col(text, ci);
                    let prev = text.split('\n').nth(line - 1).unwrap_or("");
                    let mut indent: String = prev.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                    let code = prev.split(';').next().unwrap_or("").trim();
                    if indent.is_empty() && code.ends_with(':') {
                        indent = "\t".into();
                    }
                    if !indent.is_empty() {
                        let b = char_to_byte(text, ci);
                        text.insert_str(b, &indent);
                        ci += indent.len();
                        fixed = true;
                    }
                }
                // ';' typed: move the comment to the comment column and add "; ".
                if let Some(col) = p.comment_column {
                    let typed = (now_len == prev_len + 1 && ci > 0).then(|| text.chars().nth(ci - 1)).flatten();
                    if typed == Some(';') || typed == Some(' ') {
                        let (line, c) = line_col(text, ci);
                        let l = text.split('\n').nth(line).unwrap_or("").to_string();
                        if typed == Some(';') {
                            if let Some((pad, space)) = comment_edit(&l, c - 1, col) {
                                if space {
                                    let b = char_to_byte(text, ci);
                                    text.insert(b, ' ');
                                }
                                let b = char_to_byte(text, ci - 1);
                                text.insert_str(b, &" ".repeat(pad));
                                ci += pad + usize::from(space);
                                fixed = true;
                            }
                        } else if swallow_comment_space(&l, c - 1) {
                            let b = char_to_byte(text, ci - 1);
                            text.remove(b);
                            ci -= 1;
                            fixed = true;
                        }
                    }
                }
                // Space typed right after an instruction: pad to the operand column.
                if let Some(col) = p.operand_column {
                    if now_len == prev_len + 1 && ci > 0 && text.chars().nth(ci - 1) == Some(' ') {
                        let (line, c) = line_col(text, ci);
                        let l = text.split('\n').nth(line).unwrap_or("");
                        if let Some(pad) = operand_padding(l, c - 1, col) {
                            // The typed space and the padding become tabs (and spaces for
                            // what is left when the column isn't on a tab stop).
                            let from = visual_col(l, c - 1);
                            let gap = gap_fill(from, from + 1 + pad);
                            let b = char_to_byte(text, ci - 1);
                            text.replace_range(b..b + 1, &gap);
                            ci += gap.chars().count() - 1;
                            fixed = true;
                        }
                    }
                }
                if fixed {
                    let mut st = out.state.clone();
                    st.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(ci))));
                    st.store(ui.ctx(), id);
                    result.cursor = line_col(text, ci);
                }
                // Autocomplete: a typed letter opens / refreshes the popup, a space or
                // punctuation closes it (deleting keeps it; it is re-checked next frame).
                if let Some(items) = p.completions {
                    let typed = (text.chars().count() == prev_len + 1 && ci > 0).then(|| text.chars().nth(ci - 1)).flatten();
                    autocomplete_after_typing(ui.ctx(), text, ci, typed, items);
                }
            }

            // Autocomplete popup under the word at the caret.
            if let Some(items) = p.completions {
                let mut pop = get_popup(ui.ctx());
                if pop.open && resp.has_focus() {
                    let ci = line_start(text, result.cursor.0) + result.cursor.1;
                    if let Some((c, m)) = popup_matches(text, ci, &pop, items) {
                        let row = row_rect(pop.line.min(n_lines - 1));
                        let pl = text.split('\n').nth(pop.line).unwrap_or("");
                        let (va, vb) = (visual_col(pl, c.start), visual_col(pl, c.end));
                        let x = gpos.x + va as f32 * char_w;
                        let word = Rect::from_min_max(Pos2::new(x, row.top()), Pos2::new(x + (vb - va) as f32 * char_w, row.bottom()));
                        let clicked = draw_popup(ui, word, &m, &mut pop, p.font_size);
                        if let Some(i) = clicked {
                            let ci = accept_completion(ui.ctx(), text, pop.line, &c, m[i]);
                            result.cursor = line_col(text, ci);
                            result.changed = true;
                            // The click landed outside the TextEdit: give it the focus back.
                            ui.memory_mut(|mm| mm.request_focus(id));
                        } else {
                            set_popup(ui.ctx(), pop);
                        }
                    }
                }
            }
            // While the popup is open, Esc belongs to the editor (it closes the popup) instead of
            // egui taking the focus away from the editor.
            if resp.has_focus() {
                let escape = get_popup(ui.ctx()).open;
                ui.memory_mut(|m| m.set_focus_lock_filter(id, egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape }));
            }

            // Virtual space: remember where the text is, so next frame's press past the end
            // of a line can be placed before the TextEdit sees it (virtual_space_press).
            let visible = resp.rect.intersect(ui.clip_rect());
            let prev_gpos = ui.ctx().data(|d| d.get_temp::<(Pos2, Rect)>(geom_id())).map(|g| g.0);
            ui.ctx().data_mut(|d| {
                d.insert_temp(geom_id(), (gpos, visible));
                // The height of the view itself (for PageUp / PageDown page sizes).
                d.insert_temp(editor_id().with("view_h"), ui.clip_rect().height());
            });
            // The text moved on screen (scrolled by wheel, scrollbar or caret): steady caret.
            if prev_gpos.is_some_and(|p| p != gpos) {
                hold_caret(ui.ctx());
            }

            // Background: execution line, cursor line.
            let mut bg = Vec::new();
            if let Some(cl) = cursor_ci.map(|_| result.cursor.0) {
                if resp.has_focus() {
                    bg.push(Shape::rect_filled(row_rect(cl), 0.0, Color32::from_rgb(30, 32, 37)));
                }
            }
            if let Some(pl) = p.prev_line {
                if p.exec_line.map(|e| e.0) != Some(pl) {
                    bg.push(Shape::rect_filled(row_rect(pl), 0.0, Color32::from_rgba_unmultiplied(120, 120, 255, 14)));
                }
            }
            if let Some((l, kind)) = p.exec_line {
                let (fill, edge) = match kind {
                    ExecKind::Paused => (Color32::from_rgba_unmultiplied(255, 200, 60, 34), YELLOW),
                    ExecKind::Fault => (Color32::from_rgba_unmultiplied(255, 70, 70, 44), RED),
                };
                let r = row_rect(l);
                bg.push(Shape::rect_filled(r, 0.0, fill));
                bg.push(Shape::line_segment([r.left_top(), r.left_bottom()], Stroke::new(2.0, edge)));
            }
            ui.painter().set(bg_idx, Shape::Vec(bg));

            // Gutter.
            let painter = ui.painter();
            painter.rect_filled(gutter_rect, CornerRadius::ZERO, Color32::from_rgb(25, 26, 30));
            painter.line_segment([gutter_rect.right_top(), gutter_rect.right_bottom()], Stroke::new(1.0, Color32::from_rgb(40, 42, 48)));
            let clip = ui.clip_rect();
            let mut diag_lines: HashMap<usize, Severity> = HashMap::new();
            for d in p.diags {
                let e = diag_lines.entry(d.line).or_insert(d.severity.clone());
                if d.severity == Severity::Error {
                    *e = Severity::Error;
                }
            }
            let num_font = font.clone();
            let (first, last) = if galley.rows.len() == n_lines {
                let first = galley.rows.partition_point(|r| r.pos.y + row_h < clip.top() - gpos.y).saturating_sub(1);
                let last = (galley.rows.partition_point(|r| r.pos.y <= clip.bottom() - gpos.y) + 1).min(n_lines);
                (first, last)
            } else {
                ((((clip.top() - gpos.y) / row_h).floor().max(0.0)) as usize, (((clip.bottom() - gpos.y) / row_h).ceil() as usize + 1).min(n_lines))
            };
            for line in first..last {
                let r = row_rect(line);
                let cy = digit_mid.map_or(r.center().y, |d| r.top() + d);
                let is_cur = result.cursor.0 == line && resp.has_focus();
                let is_exec = p.exec_line.map(|e| e.0) == Some(line);
                let col = if is_exec {
                    YELLOW
                } else if is_cur {
                    TEXT
                } else {
                    TEXT_FAINT
                };
                painter.text(Pos2::new(gutter_rect.left() + num_right, r.top()), egui::Align2::RIGHT_TOP, format!("{}", line + 1), num_font.clone(), col);
                // Breakpoint dot.
                let c = Pos2::new(gutter_rect.left() + BP_COL * 0.5, cy);
                let rad = (row_h * 0.3).min(5.5);
                if p.breakpoints.contains(&line) {
                    let valid = p.code_lines.is_none_or(|m| m.contains_key(&line));
                    if valid {
                        painter.circle_filled(c, rad, Color32::from_rgb(229, 20, 0));
                        painter.circle_stroke(c, rad, Stroke::new(1.0, Color32::from_rgb(255, 120, 110)));
                    } else {
                        painter.circle_stroke(c, rad - 0.5, Stroke::new(1.5, Color32::from_rgb(200, 80, 70)));
                    }
                }
                // Diagnostics marker.
                if let Some(sev) = diag_lines.get(&line) {
                    let colr = if *sev == Severity::Error { RED } else { YELLOW };
                    let m = Rect::from_min_size(Pos2::new(gutter_rect.right() - 4.0, r.top() + 2.0), egui::vec2(3.0, row_h - 4.0));
                    painter.rect_filled(m, 1.0, colr);
                }
                // Execution arrow.
                if let Some((l, kind)) = p.exec_line {
                    if l == line {
                        let x = gutter_rect.left() + arrow_left;
                        let h = (row_h * 0.3).min(ARROW_W * 0.6);
                        let fill = if kind == ExecKind::Fault { RED } else { YELLOW };
                        painter.add(Shape::convex_polygon(vec![Pos2::new(x, cy - h), Pos2::new(x + ARROW_W, cy), Pos2::new(x, cy + h)], fill, Stroke::NONE));
                    }
                }
                // Execution counts.
                if let Some(h) = p.hits.and_then(|h| h.get(&line)) {
                    if let Some(row) = galley.rows.get(line) {
                        let x = gpos.x + row.rect().right() + 18.0;
                        painter.text(Pos2::new(x, cy), egui::Align2::LEFT_CENTER, format!("×{h}"), egui::FontId::proportional(p.font_size - 3.0), Color32::from_rgb(120, 130, 160));
                    }
                }
            }

            // Gutter clicks toggle breakpoints.
            let g = ui.interact(gutter_rect, id.with("gutter"), Sense::click());
            // Map a screen y to a line using the galley's real row positions (the same ones the
            // line numbers are drawn at); `row_h` alone drifts from them over many lines.
            let line_at = |y: f32| -> usize {
                let rel = y - gpos.y;
                if galley.rows.len() == n_lines {
                    galley.rows.partition_point(|r| r.pos.y <= rel).saturating_sub(1)
                } else {
                    ((rel / row_h).floor().max(0.0) as usize).min(n_lines - 1)
                }
            };
            if g.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                if let Some(pos) = g.hover_pos() {
                    if let Some(l) = bp_target(p.breakpoints, p.bp_lines, line_at(pos.y)) {
                        if !p.breakpoints.contains(&l) {
                            let c = Pos2::new(gutter_rect.left() + BP_COL * 0.5, digit_mid.map_or(row_rect(l).center().y, |d| row_rect(l).top() + d));
                            ui.painter().circle_filled(c, (row_h * 0.3).min(5.5), Color32::from_rgba_unmultiplied(229, 20, 0, 90));
                        }
                    }
                }
            }
            if g.clicked() {
                if let Some(pos) = g.interact_pointer_pos() {
                    result.toggle_bp = bp_target(p.breakpoints, p.bp_lines, line_at(pos.y));
                }
            }

            // Hovered word.
            if let Some(pos) = resp.hover_pos() {
                let rel = pos - gpos;
                let line = line_at(pos.y);
                let within = galley.rows.get(line).is_some_and(|r| rel.x <= r.rect().right() + 2.0);
                if within {
                    let cc = galley.cursor_from_pos(rel);
                    let ci = cc.index.0;
                    let chars: Vec<char> = text.chars().collect();
                    let is_w = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '$' || c == '?' || c == '@' || c == '\'';
                    let mut s = ci.min(chars.len());
                    // Prefer the char under the pointer (cursor_from_pos rounds to the nearest gap).
                    if s < chars.len() && !is_w(chars[s]) && s > 0 {
                        s -= 1;
                    }
                    if s < chars.len() && is_w(chars[s]) {
                        let mut a = s;
                        while a > 0 && is_w(chars[a - 1]) {
                            a -= 1;
                        }
                        let mut b = s;
                        while b < chars.len() && is_w(chars[b]) {
                            b += 1;
                        }
                        let word: String = chars[a..b].iter().collect();
                        let (wl, _) = line_col(text, a);
                        // Not inside a comment.
                        let l = text.split('\n').nth(wl).unwrap_or("");
                        let col_in_line = a - line_start(text, wl);
                        let comment_at = l.char_indices().find(|(_, c)| *c == ';').map(|(b, _)| l[..b].chars().count());
                        if comment_at.is_none_or(|c| col_in_line < c) {
                            result.hover = Some(Hover { word, line: wl });
                        }
                    }
                }
            }

            // Right-click on a line: context actions.
            resp.context_menu(|ui| {
                let l = result.cursor.0;
                if let Some(b) = bp_target(p.breakpoints, p.bp_lines, l) {
                    if ui.button(format!("Toggle breakpoint (line {})", b + 1)).clicked() {
                        result.toggle_bp = Some(b);
                        ui.close();
                    }
                }
                if ui.button("Run to cursor").clicked() {
                    result.run_to = Some(l);
                    ui.close();
                }
            });

            if let Some(line) = p.scroll_to {
                ui.scroll_to_rect(row_rect(line.min(n_lines - 1)).with_max_x(gutter_rect.right() + 200.0), Some(Align::Center));
                redraw_scrolled(ui.ctx());
            } else if resp.has_focus() {
                // Keep the caret in view whenever it moves. The TextEdit only does this for
                // moves it makes itself, and Up / Down / Right are handled before it
                // (virtual space), so it never sees those.
                let caret = result.cursor;
                let last_id = id.with("last_caret");
                let last = ui.ctx().data(|d| d.get_temp::<(usize, usize)>(last_id));
                if last != Some(caret) {
                    ui.ctx().data_mut(|d| d.insert_temp(last_id, caret));
                    hold_caret(ui.ctx());
                    if let Some(old_line) = page_from {
                        // PageUp / PageDown: the text scrolls by exactly as much as the caret
                        // moved, so the caret keeps its height on screen (the view stops at the
                        // ends of the text).
                        let delta = row_rect(caret.0.min(n_lines - 1)).top() - row_rect(old_line.min(n_lines - 1)).top();
                        let clip = ui.clip_rect();
                        let new_top = Rect::from_min_size(Pos2::new(gutter_rect.left(), clip.top() + delta), egui::vec2(1.0, 1.0));
                        ui.scroll_to_rect(new_top, Some(Align::TOP));
                        redraw_scrolled(ui.ctx());
                    } else if last.is_some() {
                        let (line, col) = caret;
                        let x = gpos.x + visual_col_at(text, line, col) as f32 * char_w;
                        let row = row_rect(line.min(n_lines - 1));
                        // Scroll only when the caret line itself isn't fully visible (a line at the
                        // edge can be reached without scrolling); sideways a few columns of margin.
                        let r = Rect::from_min_max(Pos2::new(x - 4.0 * char_w, row.top()), Pos2::new(x + 4.0 * char_w, row.bottom()));
                        ui.scroll_to_rect(r, None);
                        let clip = ui.clip_rect();
                        if !clip.contains_rect(r) {
                            redraw_scrolled(ui.ctx());
                        }
                    }
                }
            }
            result.response = Some(resp);
        });
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_toggle() {
        let mut t = String::from("  mov eax, 1\n  add eax, 2\nx");
        toggle_comment(&mut t, 0, 1);
        assert_eq!(t, "  ; mov eax, 1\n  ; add eax, 2\nx");
        toggle_comment(&mut t, 0, 1);
        assert_eq!(t, "  mov eax, 1\n  add eax, 2\nx");
    }
}

#[cfg(test)]
mod indent_tests {
    use super::*;

    #[test]
    fn indent_and_unindent() {
        let mut t = String::from("mov eax, 1\n\n  add eax, 2");
        indent_lines(&mut t, 0, 2, true);
        assert_eq!(t, "\tmov eax, 1\n\n\t  add eax, 2");
        indent_lines(&mut t, 0, 2, false);
        assert_eq!(t, "mov eax, 1\n\n  add eax, 2");
        indent_lines(&mut t, 2, 2, false);
        assert_eq!(t, "mov eax, 1\n\nadd eax, 2");
        // Spaces then a tab: one step removes up to and including the tab.
        let mut t = String::from("  \tnop\n        nop");
        indent_lines(&mut t, 0, 1, false);
        assert_eq!(t, "nop\n    nop");
    }

    #[test]
    fn tab_columns() {
        assert_eq!(visual_col("\tmov", 1), 4);
        assert_eq!(visual_col("\t\tmov", 2), 8);
        assert_eq!(visual_col("ab\tc", 3), 4);
        assert_eq!(visual_col("abcd\tc", 5), 8);
        assert_eq!(char_col("\tmov", 4), (1, 0));
        assert_eq!(char_col("\tmov", 1), (0, 0)); // inside the tab: nearer edge
        assert_eq!(char_col("\tmov", 3), (1, 0));
        assert_eq!(char_col("\tmov", 10), (4, 3));
        let mut t = String::from("\tnop\n");
        assert_eq!(pad_line(&mut t, 0, 9), 2);
        assert_eq!(t, "\tnop  \n");
    }

    #[test]
    fn tabify_text() {
        let t = tabify("label:\n    mov eax, 1\n        nop\n      x\n    \n\tret\n  \tz");
        assert_eq!(t, "label:\n\tmov eax, 1\n\t\tnop\n\t  x\n    \n\tret\n\tz");
        // The gap after an instruction, when it aligns the operands.
        assert_eq!(tabify_line("    mov     eax, 1  ; hi"), "\tmov\t\teax, 1  ; hi");
        assert_eq!(tabify_line("    movzx   eax, bl"), "\tmovzx\teax, bl");
        assert_eq!(tabify_line("    pushad"), "\tpushad");
        assert_eq!(tabify_line("    int     0x80"), "\tint\t\t0x80");
        assert_eq!(tabify_line("    cmovne eax, ebx"), "\tcmovne eax, ebx"); // single space: kept
        assert_eq!(tabify_line("    rep     movsb"), "\trep     movsb"); // prefix
        assert_eq!(tabify_line("    t_sq    db  \"x\", 0"), "\tt_sq    db  \"x\", 0"); // data
        assert_eq!(tabify_line("    ret     ; done"), "\tret     ; done"); // no operands
        assert_eq!(tabify_line("    mov       eax, 1"), "\tmov\t\t  eax, 1"); // column 14
        assert_eq!(gap_fill(7, 12), "\t\t");
        assert_eq!(gap_fill(10, 13), "\t ");
        assert_eq!(gap_fill(11, 12), "\t");
        assert_eq!(map_caret("    mov", "\tmov", (0, 5)), (0, 2));
        assert_eq!(map_caret("    mov", "\tmov", (0, 2)), (0, 0));
    }

    #[test]
    fn tab_aware_padding() {
        // "\tmov " has its space at visual column 7.
        assert_eq!(operand_padding("\tmov ", 4, 12), Some(4));
        assert_eq!(comment_edit("\tnop;", 4, 28), Some((21, true)));
    }
}

#[cfg(test)]
mod operand_tests {
    use super::operand_padding;

    #[test]
    fn pads_after_mnemonic() {
        // "    mov" + typed space at col 7 -> operands at col 12: 4 more spaces.
        assert_eq!(operand_padding("    mov ", 7, 12), Some(4));
        assert_eq!(operand_padding("loop1: inc ", 10, 12), Some(1));
        assert_eq!(operand_padding("    pushad ", 10, 12), Some(1));
        // Long mnemonic past the column: next tab stop.
        assert_eq!(operand_padding("    cmovne ", 10, 8), Some(1));
        assert_eq!(operand_padding("        movzx ", 13, 12), Some(2));
    }

    #[test]
    fn leaves_other_spaces_alone() {
        assert_eq!(operand_padding("    mov eax, ", 12, 12), None); // after operands
        assert_eq!(operand_padding("    msg ", 7, 12), None); // not a mnemonic
        assert_eq!(operand_padding("    rep ", 7, 12), None); // prefix
        assert_eq!(operand_padding("    db ", 6, 12), None); // directive
        assert_eq!(operand_padding("    mov x", 7, 12), None); // text after the cursor
        assert_eq!(operand_padding("    mov  ", 8, 12), None); // second space
    }
}

/// Drives the real editor widget headlessly (no window, no OS input) with egui events.
#[cfg(test)]
mod headless_tests {
    use super::*;
    use egui::{Event, Key, Modifiers, RawInput};

    fn key(k: Key, modifiers: Modifiers) -> Event {
        Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    /// Starts with `start`, caret at the end, then feeds each frame's events.
    fn type_frames(start: &str, frames: Vec<Vec<Event>>, operand_column: Option<usize>) -> String {
        type_frames_c(start, frames, operand_column, None)
    }

    fn type_frames_c(start: &str, frames: Vec<Vec<Event>>, operand_column: Option<usize>, comment_column: Option<usize>) -> String {
        run_frames(start, frames, operand_column, comment_column).0
    }

    /// Like type_frames_c, but also returns the egui context (to inspect editor state).
    fn run_frames(start: &str, frames: Vec<Vec<Event>>, operand_column: Option<usize>, comment_column: Option<usize>) -> (String, egui::Context) {
        run_frames_ac(start, frames, operand_column, comment_column, false)
    }

    /// `autocomplete`: with the popup enabled (instructions, registers, ... no symbols).
    fn run_frames_ac(start: &str, frames: Vec<Vec<Event>>, operand_column: Option<usize>, comment_column: Option<usize>, autocomplete: bool) -> (String, egui::Context) {
        let items = complete::all_items(&[]);
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        let mono = fonts.families[&egui::FontFamily::Monospace].clone();
        fonts.families.insert(code(), mono);
        ctx.set_fonts(fonts);
        let mut text = start.to_string();
        let bps = BTreeSet::new();
        let t = std::cell::Cell::new(0.0);
        let frame = |ctx: &egui::Context, text: &mut String, events: Vec<Event>| {
            t.set(t.get() + 0.1); // time moves on, so scroll animations finish
            let input = RawInput { events, time: Some(t.get()), screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0))), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                let p = Params { font_size: 14.0, breakpoints: &bps, code_lines: None, bp_lines: None, exec_line: None, prev_line: None, diags: &[], scroll_to: None, hits: None, operand_column, comment_column, virtual_space: true, italic_comments: true, completions: autocomplete.then_some(&items[..]) };
                show(ui, text, &p);
            });
            out.textures_delta.clear();
        };
        frame(&ctx, &mut text, vec![]);
        ctx.memory_mut(|m| m.request_focus(editor_id()));
        let mut st = egui::TextEdit::load_state(&ctx, editor_id()).unwrap_or_default();
        st.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(text.chars().count()))));
        st.store(&ctx, editor_id());
        frame(&ctx, &mut text, vec![]);
        for evs in frames {
            frame(&ctx, &mut text, evs);
        }
        (text, ctx)
    }

    fn txt(s: &str) -> Event {
        Event::Text(s.into())
    }

    /// Types `start`'s continuation one character per frame, then the given keys, with
    /// autocomplete on.
    fn ac(start: &str, typed: &str, keys: &[Key]) -> String {
        let mut frames: Vec<Vec<Event>> = typed.chars().map(|c| vec![txt(&c.to_string())]).collect();
        frames.extend(keys.iter().map(|k| vec![key(*k, Modifiers::NONE)]));
        run_frames_ac(start, frames, None, None, true).0
    }

    #[test]
    fn autocomplete_inserts_the_chosen_item() {
        // "mo" + Enter: the first match, instead of a new line.
        assert_eq!(ac("", "mo", &[Key::Enter]), "mov");
        // Down picks the next one (alphabetical: mov, movsb, ...); Tab inserts too.
        assert_eq!(ac("", "m", &[Key::ArrowDown, Key::Tab]), "movsb");
        // Operands: registers.
        assert_eq!(ac("mov ", "eb", &[Key::Enter]), "mov ebp");
        // Upper case typing gives upper case.
        assert_eq!(ac("", "MO", &[Key::Enter]), "MOV");
    }

    #[test]
    fn autocomplete_esc_and_punctuation_close_it() {
        // Esc closes it: Enter then makes a new line as usual.
        assert_eq!(ac("", "mo", &[Key::Escape, Key::Enter]), "mo\n");
        // A space ends the word: Enter is a new line again.
        assert_eq!(ac("", "mov ", &[Key::Enter]), "mov \n");
        // Nothing offered inside a comment.
        assert_eq!(ac("; ", "mo", &[Key::Enter]), "; mo\n");
    }

    #[test]
    fn moving_to_a_visible_line_does_not_scroll() {
        let text: String = (0..200).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let up = || vec![key(Key::ArrowUp, Modifiers::NONE)];
        // Up once: the view follows the caret to the bottom of the file.
        let (_, ctx) = run_frames(&text, vec![up()], None, None);
        let (gpos0, visible) = ctx.data(|d| d.get_temp::<(Pos2, Rect)>(geom_id())).unwrap();
        let rows = ctx.fonts_mut(|f| f.layout_no_wrap(text.clone(), egui::FontId::new(14.0, code()), Color32::WHITE)).rows.iter().map(|r| r.pos.y).collect::<Vec<_>>();
        // The first line that is fully visible at the top.
        let first = (0..200).find(|&i| gpos0.y + rows[i] >= visible.top()).unwrap();
        // Up to that line: nothing scrolls on the way.
        let frames: Vec<_> = (0..(1 + 198 - first)).map(|_| up()).collect();
        let (t, ctx) = run_frames(&text, frames, None, None);
        let (gpos1, _) = ctx.data(|d| d.get_temp::<(Pos2, Rect)>(geom_id())).unwrap();
        let caret = egui::TextEdit::load_state(&ctx, editor_id()).unwrap().cursor.char_range().unwrap().primary.index.0;
        assert_eq!(line_col(&t, caret).0, first);
        assert_eq!(gpos1.y, gpos0.y, "the top visible line was reached without scrolling");
        // One more Up: now it scrolls (by about a line).
        let frames: Vec<_> = (0..(2 + 198 - first)).map(|_| up()).collect();
        let (_, ctx) = run_frames(&text, frames, None, None);
        let (gpos2, _) = ctx.data(|d| d.get_temp::<(Pos2, Rect)>(geom_id())).unwrap();
        assert!(gpos2.y > gpos0.y, "moving above the view scrolls");
    }

    #[test]
    fn page_up_and_down_move_a_screenful() {
        let text: String = (0..200).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let k = |key_: Key, m: Modifiers| vec![key(key_, m)];
        // (caret line, caret height on screen, selection?) after these keys.
        let after = |keys: Vec<Vec<Event>>| {
            let (t, ctx) = run_frames(&text, keys, None, None);
            let st = egui::TextEdit::load_state(&ctx, editor_id()).unwrap();
            let r = st.cursor.char_range().unwrap();
            let line = line_col(&t, r.primary.index.0).0;
            let (gpos, visible) = ctx.data(|d| d.get_temp::<(Pos2, Rect)>(geom_id())).unwrap();
            let row_h = ctx.fonts_mut(|f| f.row_height(&egui::FontId::new(14.0, code())));
            let row_y = ctx.fonts_mut(|f| f.layout_no_wrap(t.clone(), egui::FontId::new(14.0, code()), Color32::WHITE).rows[line].pos.y);
            let y = gpos.y + row_y - visible.top();
            let view_h = ctx.data(|d| d.get_temp::<f32>(editor_id().with("view_h"))).unwrap();
            (line, y, r.primary != r.secondary, ((view_h / row_h).floor() as usize).max(2) - 1)
        };
        // Up first, so the view follows the caret to the bottom of the file.
        let (l0, y0, _, page) = after(vec![k(Key::ArrowUp, Modifiers::NONE), vec![]]);
        assert_eq!(l0, 198);
        let (l1, y1, sel, _) = after(vec![k(Key::ArrowUp, Modifiers::NONE), vec![], k(Key::PageUp, Modifiers::NONE), vec![]]);
        assert_eq!(l1, 198 - page, "PageUp moves one page up");
        assert!((y1 - y0).abs() < 20.0, "and the text scrolls with it: the caret keeps its height ({y0} -> {y1})");
        assert!(!sel);
        let (l2, _, _, _) = after(vec![k(Key::ArrowUp, Modifiers::NONE), vec![], k(Key::PageUp, Modifiers::NONE), vec![], k(Key::PageDown, Modifiers::NONE), vec![]]);
        assert_eq!(l2, 198, "PageDown goes back");
        // Shift: selects.
        let (_, _, sel, _) = after(vec![k(Key::ArrowUp, Modifiers::NONE), vec![], k(Key::PageUp, Modifiers::SHIFT), vec![]]);
        assert!(sel, "Shift+PageUp selects");
        // At the top: PageUp goes to the first line.
        let (l3, _, _, _) = after((0..12).flat_map(|_| [k(Key::PageUp, Modifiers::NONE), vec![]]).collect());
        assert_eq!(l3, 0);
    }

    #[test]
    fn caret_moved_by_up_down_is_scrolled_into_view() {
        // 200 lines, caret on the last one (out of view in an 800 px window). Up is handled
        // by the virtual-space code before the TextEdit, which used to leave the view behind.
        let text: String = (0..200).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let frames = vec![vec![key(Key::ArrowUp, Modifiers::NONE)]];
        // Nothing more: the frame of the key press itself already shows the caret line (it is
        // redrawn with the new scroll position, never shown with the old one).
        let (_, ctx) = run_frames(&text, frames, None, None);
        let (gpos, visible) = ctx.data(|d| d.get_temp::<(Pos2, Rect)>(geom_id())).unwrap();
        // Where row 198 really is in the laid-out text (rows are placed on whole pixels).
        let font = egui::FontId::new(14.0, code());
        let (row_y, row_h) = ctx.fonts_mut(|f| { let g = f.layout_no_wrap(text.clone(), font.clone(), Color32::WHITE); (g.rows[198].pos.y, f.row_height(&font)) });
        let caret_y = gpos.y + row_y;
        assert!(visible.top() <= caret_y && caret_y + row_h <= visible.bottom(), "caret row at y={caret_y}, visible {visible:?}");
    }

    #[test]
    fn tab_goes_to_tab_stops() {
        let t = type_frames("x\n", vec![vec![txt("int")], vec![key(Key::Tab, Modifiers::NONE)], vec![key(Key::Tab, Modifiers::NONE)], vec![txt("0x80")]], None);
        assert_eq!(t, "x\nint\t\t0x80");
        // At the start of a line too, and after a label line Enter indents with a tab.
        let t = type_frames("x", vec![vec![txt(":")], vec![key(Key::Enter, Modifiers::NONE)], vec![txt("nop")]], None);
        assert_eq!(t, "x:\n\tnop");
    }

    #[test]
    fn arrows_step_over_a_tab() {
        let left = || vec![key(Key::ArrowLeft, Modifiers::NONE)];
        let right = || vec![key(Key::ArrowRight, Modifiers::NONE)];
        // From the end: four Lefts reach the line start (the tab is one step), one Right is
        // past the whole tab.
        let t = type_frames("\tnop", vec![left(), left(), left(), left(), vec![txt("|")]], None);
        assert_eq!(t, "|\tnop");
        let t = type_frames("\tnop", vec![left(), left(), left(), left(), right(), vec![txt("|")]], None);
        assert_eq!(t, "\t|nop");
    }

    #[test]
    fn shift_tab_unindents() {
        let t = type_frames("      mov", vec![vec![key(Key::Tab, Modifiers::SHIFT)]], None);
        assert_eq!(t, "  mov");
    }

    #[test]
    fn virtual_space_up_down_keeps_column() {
        let up = || vec![key(Key::ArrowUp, Modifiers::NONE)];
        let down = || vec![key(Key::ArrowDown, Modifiers::NONE)];
        // From column 6 up onto an empty line, then type: spaces become real.
        let t = type_frames("abc\n\nabcdef", vec![up(), vec![txt("z")]], None);
        assert_eq!(t, "abc\n      z\nabcdef");
        // Up twice and back down twice without typing: no spaces are left behind.
        let t = type_frames("abc\n\nabcdef", vec![up(), up(), down(), down(), vec![txt("!")]], None);
        assert_eq!(t, "abc\n\nabcdef!");
    }

    #[test]
    fn virtual_space_click_past_end() {
        let at = Pos2::new(900.0, 12.0); // far right of line 0
        let press = |pressed| Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        let t = type_frames("ab\ncd", vec![vec![Event::PointerMoved(at), press(true)], vec![press(false)], vec![txt("x")]], None);
        let first = t.lines().next().unwrap();
        assert!(first.starts_with("ab") && first.ends_with('x'), "{first:?}");
        assert!(first.len() > 20, "the cursor went to where it was clicked: {first:?}");
        assert!(first[2..first.len() - 1].chars().all(|c| c == ' '));
    }

    #[test]
    fn virtual_space_press_places_cursor_before_release() {
        // The caret must go to the pointer on press, not jump to the line end first.
        let at = Pos2::new(900.0, 12.0);
        let press = Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE };
        let t = type_frames("ab\ncd", vec![vec![Event::PointerMoved(at), press], vec![], vec![txt("x")]], None);
        let first = t.lines().next().unwrap();
        assert!(first.len() > 20 && first.ends_with('x'), "{first:?}");
        assert!(first[2..first.len() - 1].chars().all(|c| c == ' '));
    }

    #[test]
    fn virtual_space_right_past_end() {
        let right = || vec![key(Key::ArrowRight, Modifiers::NONE)];
        let t = type_frames("x\nnop", vec![right(), right(), right(), vec![txt("1")]], None);
        assert_eq!(t, "x\nnop   1");
    }

    #[test]
    fn semicolon_moves_comment() {
        // The space after ';' is added automatically...
        let t = type_frames_c("x:\n    int     0x80", vec![vec![txt(";")], vec![txt("exit")]], None, Some(28));
        assert_eq!(t, "x:\n    int     0x80            ; exit");
        // ...and typing one anyway doesn't double it.
        let t = type_frames_c("x:\n    int     0x80", vec![vec![txt(";")], vec![txt(" ")], vec![txt("exit")]], None, Some(28));
        assert_eq!(t, "x:\n    int     0x80            ; exit");
        // Comment-only line: "; " right where it was typed.
        let t = type_frames_c("x:\n    ", vec![vec![txt(";")], vec![txt("note")]], None, Some(28));
        assert_eq!(t, "x:\n    ; note");
        // Off: the ';' stays where it was typed.
        let t = type_frames_c("x:\n    nop", vec![vec![txt(";")]], None, None);
        assert_eq!(t, "x:\n    nop;");
    }

    #[test]
    fn space_after_mnemonic_aligns_operands() {
        let t = type_frames("x:\n    ", vec![vec![txt("mov")], vec![txt(" ")], vec![txt("eax,")], vec![txt(" ")], vec![txt("4")]], Some(12));
        assert_eq!(t, "x:\n    mov\t\teax, 4");
        let t = type_frames("x:\n\t", vec![vec![txt("movzx")], vec![txt(" ")], vec![txt("eax")]], Some(12));
        assert_eq!(t, "x:\n\tmovzx\teax");
        // Setting off: a plain space.
        let t = type_frames("x:\n    ", vec![vec![txt("mov")], vec![txt(" ")], vec![txt("eax")]], None);
        assert_eq!(t, "x:\n    mov eax");
    }
}

#[cfg(test)]
mod comment_tests {
    use super::{comment_edit, swallow_comment_space};

    #[test]
    fn moves_comment_to_column_and_adds_space() {
        // "    int     0x80" is 16 chars; ';' typed at col 16 -> column 28: 12 spaces.
        assert_eq!(comment_edit("    int     0x80;", 16, 28), Some((12, true)));
        assert_eq!(comment_edit("    int     0x80   ;", 19, 28), Some((9, true)));
        // Code already past the column: just one separating space.
        assert_eq!(comment_edit("    mov     eax, [very_long_label + 4];", 38, 28), Some((1, true)));
        assert_eq!(comment_edit("    mov     eax, [very_long_label + 4] ;", 39, 28), Some((0, true)));
        // Comment-only line: stays put, still gets "; ".
        assert_eq!(comment_edit("    ;", 4, 28), Some((0, true)));
    }

    #[test]
    fn leaves_other_semicolons_alone() {
        assert_eq!(comment_edit("msg db \"a;", 9, 28), None); // inside a string
        assert_eq!(comment_edit("    nop ; note;", 14, 28), None); // already a comment
        assert_eq!(comment_edit("    nop; x", 7, 28), None); // text after the cursor
    }

    #[test]
    fn habitual_space_after_semicolon_is_swallowed() {
        assert!(swallow_comment_space("    nop ;  ", 10));
        assert!(!swallow_comment_space("    nop ; x ", 11)); // inside comment text
        assert!(!swallow_comment_space("    db \";  ", 10)); // inside a string
    }
}
