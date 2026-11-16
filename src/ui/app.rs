//! The main window.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use egui::{Align, Color32, Key, Layout, Modifiers, RichText, Sense, Stroke, Ui};
use serde::{Deserialize, Serialize};

use super::editor::{self, ExecKind};
use super::widgets::{self, Icon};
use super::{filedlg, theme::*};
use crate::asm::{self, Diagnostic, Program, Reg, Severity, Size, SymKind, CODE_BASE, DATA_BASE, EXIT_ADDR, MEM_SIZE, SCREEN_BASE, SCREEN_END, SCREEN_W, STACK_TOP};
use crate::cpu::{Machine, Status};
use crate::debugger::{Debugger, Mode, Stop};
use crate::{docs, examples};

mod update_ui;

/// Speed slider value meaning "as fast as possible".
const MAX_SPEED: f32 = 1001.0;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum NumFmt {
    Hex,
    Dec,
    Signed,
    Bin,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum BottomTab {
    Console,
    Problems,
    Memory,
    Symbols,
    Reference,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum RefTab {
    Instructions,
    Syscalls,
    Registers,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub source: String,
    pub file: Option<PathBuf>,
    /// Name shown for an unsaved document (e.g. a built-in example); None = "untitled.asm".
    pub doc_name: Option<String>,
    pub dirty: bool,
    /// The text as last loaded / saved: the document is dirty only while it differs, so
    /// undoing every edit clears the unsaved mark again.
    pub saved_source: String,
    pub breakpoints: Vec<usize>,
    pub code_size: f32,
    pub ui_size: f32,
    pub code_font: String,
    pub speed: f32,
    pub num_fmt: NumFmt,
    pub show_hits: bool,
    pub align_operands: bool,
    pub operand_column: usize,
    pub screen_open: bool,
    pub align_comments: bool,
    pub virtual_space: bool,
    /// Show the autocomplete popup while typing.
    pub autocomplete: bool,
    /// Draw comments in italics.
    pub italic_comments: bool,
    pub comment_column: usize,
    pub bottom_tab: BottomTab,
    pub recent: Vec<PathBuf>,
    pub ref_selected: String,
    /// The other open tabs (the active one is the fields above), in tab order without it,
    /// and where the active one goes. Filled in when the settings are saved.
    pub other_tabs: Vec<SavedDoc>,
    pub active_tab: usize,
    /// Caret (line, column) of the active document, so it is back in place after a restart.
    pub caret: (usize, usize),
    /// Check GitHub for a new release at start-up and every few hours (Help menu).
    pub auto_update: bool,
}

/// A document as it is saved with the settings (one per tab).
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct SavedDoc {
    pub source: String,
    pub file: Option<PathBuf>,
    pub doc_name: Option<String>,
    pub dirty: bool,
    pub saved_source: String,
    pub breakpoints: Vec<usize>,
    /// Where the caret was (line, column).
    pub caret: (usize, usize),
}

/// Text of a new document.
const NEW_TEMPLATE: &str = "section .data\n\nsection .text\nglobal _start\n\n_start:\n\t\n\n\tmov eax, 1      ; exit(0)\n\txor ebx, ebx\n\tint 0x80\n";

impl SavedDoc {
    /// Indents with tabs (keeping the caret on the same character).
    fn tabify(&mut self) {
        let src = editor::tabify(&self.source);
        self.caret = editor::map_caret(&self.source, &src, self.caret);
        self.source = src;
        self.saved_source = editor::tabify(&self.saved_source);
    }
}

fn new_doc() -> SavedDoc {
    SavedDoc { source: NEW_TEMPLATE.into(), saved_source: NEW_TEMPLATE.into(), ..Default::default() }
}

/// A tab that isn't being shown: its document and everything that belongs to it.
struct Parked {
    doc: SavedDoc,
    bps: BTreeSet<usize>,
    dbg: Option<Debugger>,
    /// The session of a program that finished (exited): no longer debuggable, but its console
    /// output and screen stay visible until the next run.
    last_run: Option<Debugger>,
    session_src: String,
    stop_msg: Option<(String, Color32)>,
    cursor: (usize, usize),
    screen_auto_opened: bool,
    /// Caret, selection and undo history of the editor.
    editor_state: Option<egui::text_edit::TextEditState>,
}

impl Parked {
    fn from_doc(doc: SavedDoc) -> Parked {
        Parked {
            bps: doc.breakpoints.iter().copied().collect(),
            dbg: None,
            last_run: None,
            session_src: String::new(),
            stop_msg: None,
            cursor: doc.caret,
            screen_auto_opened: false,
            // Not shown yet: the caret goes where it was last time (the start for a new one),
            // not where egui would put it (the end).
            editor_state: Some(caret_state(&doc.source, doc.caret)),
            doc,
        }
    }
}

/// Editor state with only the caret set, at (line, column) of `text` (clamped to the text).
fn caret_state(text: &str, (line, col): (usize, usize)) -> egui::text_edit::TextEditState {
    let mut ci = 0;
    for (i, l) in text.split('\n').enumerate() {
        let len = l.chars().count();
        if i == line {
            ci += col.min(len);
            break;
        }
        ci += len + 1;
    }
    let mut st = egui::text_edit::TextEditState::default();
    st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(ci.min(text.chars().count())))));
    st
}

/// An open document. The active tab's document lives in the App's own fields (`parked` is
/// None); the others are parked.
struct Tab {
    id: u64,
    parked: Option<Parked>,
}

impl Default for Settings {
    fn default() -> Self {
        let first = examples::list().into_iter().next();
        let source = editor::tabify(&first.as_ref().map(|e| e.source.replace("\r\n", "\n")).unwrap_or_else(|| DEFAULT_SOURCE.to_string()));
        Settings {
            source,
            file: None,
            doc_name: first.map(|e| e.file_name.to_string()),
            dirty: false,
            saved_source: String::new(), // filled in by App::new
            breakpoints: Vec::new(),
            code_size: 15.0,
            ui_size: 14.0,
            code_font: "Cascadia Mono".into(),
            speed: MAX_SPEED,
            num_fmt: NumFmt::Hex,
            show_hits: false,
            align_operands: true,
            operand_column: 12,
            screen_open: false,
            align_comments: true,
            virtual_space: true,
            autocomplete: true,
            italic_comments: true,
            comment_column: 28,
            bottom_tab: BottomTab::Console,
            recent: Vec::new(),
            ref_selected: "mov".into(),
            other_tabs: Vec::new(),
            active_tab: 0,
            caret: (0, 0),
            auto_update: true,
        }
    }
}

const DEFAULT_SOURCE: &str = "section .data\n    msg db \"Hello!\", 10\n    len equ $ - msg\n\nsection .text\nglobal _start\n\n_start:\n    mov eax, 4\n    mov ebx, 1\n    mov ecx, msg\n    mov edx, len\n    int 0x80\n\n    mov eax, 1\n    xor ebx, ebx\n    int 0x80\n";

enum Pending {
    New,
    Open,
    OpenPath(PathBuf),
    /// A built-in example, by file name.
    OpenExample(&'static str),
    /// Close the active tab.
    CloseTab,
}

pub struct App {
    s: Settings,
    bps: BTreeSet<usize>,
    diags: Vec<Diagnostic>,
    dbg: Option<Debugger>,
    /// The session of a program that finished (exited): no longer debuggable, but its console
    /// output and screen stay visible until the next run.
    last_run: Option<Debugger>,
    /// Source the current session was assembled from.
    session_src: String,
    scroll_to: Option<usize>,
    console_input: String,
    focus_console: bool,
    mem_goto: String,
    mem_jump: Option<u32>,
    budget: f64,
    last_frame: Instant,
    toast: Option<(String, Color32, f64)>,
    applied: (String, f32),
    cursor: (usize, usize),
    about: bool,
    confirm: Option<Pending>,
    ref_filter: String,
    example_list: Vec<examples::Example>,
    tutorial_list: Vec<examples::Example>,
    advanced_list: Vec<examples::Example>,
    /// Autocomplete candidates: instructions, registers, ... plus the program's own symbols.
    completions: Vec<super::complete::Item>,
    /// Instructions and bytes of data of the program as it is now (from the last check).
    prog_size: (usize, u32),
    /// Lines that hold an instruction (from the last check): breakpoints snap to these.
    instr_lines: BTreeSet<usize>,
    stop_msg: Option<(String, Color32)>,
    /// The app icon, shown at the left of the toolbar.
    logo: Option<egui::TextureHandle>,
    /// Large icon for the About dialog, loaded the first time it opens.
    about_icon: Option<egui::TextureHandle>,
    screen: super::screen::ScreenView,
    /// The Screen window was opened automatically for this session already.
    screen_auto_opened: bool,
    /// Start running on the first frame (--run on the command line).
    run_on_start: bool,
    /// Give the editor keyboard focus at the start of the next frame (on start-up, so you can
    /// type right away, and after a tab click).
    focus_editor_on_start: bool,
    /// Open documents in tab order; exactly one (the active one) has no parked state.
    tabs: Vec<Tab>,
    next_tab_id: u64,
    /// Tab ids, most recently used first (Ctrl+Tab goes down this list).
    mru: Vec<u64>,
    /// While Ctrl is held after Ctrl+Tab: the MRU list as it was, and how far down it we are.
    tab_switch: Option<(Vec<u64>, usize)>,
    /// The Check for updates dialog is open.
    update_dialog: bool,
    /// The release whose "downloaded" toast was shown.
    update_announced: Option<String>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        let s: Settings = cc.storage.and_then(|st| eframe::get_value(st, eframe::APP_KEY)).unwrap_or_default();
        install_fonts(&cc.egui_ctx, &s.code_font);
        apply_style(&cc.egui_ctx, s.ui_size);
        let applied = (s.code_font.clone(), s.ui_size);
        let bps = s.breakpoints.iter().copied().collect();
        let mut app = App {
            s,
            bps,
            diags: Vec::new(),
            dbg: None,
            last_run: None,
            session_src: String::new(),
            scroll_to: None,
            console_input: String::new(),
            focus_console: false,
            mem_goto: String::new(),
            mem_jump: None,
            budget: 0.0,
            last_frame: Instant::now(),
            toast: None,
            applied,
            cursor: (0, 0),
            about: false,
            confirm: None,
            ref_filter: String::new(),
            example_list: examples::list(),
            tutorial_list: examples::tutorial(),
            advanced_list: examples::advanced(),
            completions: Vec::new(),
            prog_size: (0, 0),
            instr_lines: BTreeSet::new(),
            stop_msg: None,
            screen: super::screen::ScreenView::new(),
            screen_auto_opened: false,
            run_on_start: false,
            focus_editor_on_start: true,
            tabs: vec![Tab { id: 1, parked: None }],
            next_tab_id: 1,
            mru: vec![1],
            tab_switch: None,
            update_dialog: false,
            update_announced: None,
            about_icon: None,
            logo: eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon_64.png")).ok().map(|icon| {
                let img = egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba);
                cc.egui_ctx.load_texture("app_logo", img, egui::TextureOptions::LINEAR)
            }),
        };
        // Forget recent files that no longer exist (moved, renamed or deleted).
        app.s.recent.retain(|p| p.is_file());
        // Settings from before saved_source existed: a clean document is its own saved text; a
        // dirty one is compared with its file on disk, or the built-in example it came from.
        if app.s.saved_source.is_empty() {
            let original = if !app.s.dirty {
                Some(app.s.source.clone())
            } else if let Some(f) = &app.s.file {
                std::fs::read_to_string(f).ok()
            } else {
                app.s.doc_name.as_deref().and_then(examples::source).map(str::to_string)
            };
            if let Some(text) = original {
                app.s.saved_source = text.replace("\r\n", "\n");
                app.s.dirty = !same_text(&app.s.source, &app.s.saved_source);
            }
        }
        // Documents from last time (saved when indentation was spaces): indent with tabs.
        let src = editor::tabify(&app.s.source);
        app.s.caret = editor::map_caret(&app.s.source, &src, app.s.caret);
        app.s.source = src;
        app.s.saved_source = editor::tabify(&app.s.saved_source);
        for d in &mut app.s.other_tabs {
            d.tabify();
        }
        // The active document's caret from last time (unless egui kept the editor state).
        app.cursor = app.s.caret;
        if egui::TextEdit::load_state(&cc.egui_ctx, editor::editor_id()).is_none() {
            caret_state(&app.s.source, app.s.caret).store(&cc.egui_ctx, editor::editor_id());
        }
        // The other tabs from last time, around the active one.
        let others = std::mem::take(&mut app.s.other_tabs);
        let active_pos = app.s.active_tab.min(others.len());
        for (i, doc) in others.into_iter().enumerate() {
            let id = app.new_tab_id();
            let at = if i < active_pos { i } else { i + 1 };
            app.tabs.insert(at.min(app.tabs.len()), Tab { id, parked: Some(Parked::from_doc(doc)) });
            app.mru.push(id);
        }
        // Command line: [file.asm] [--run]
        let ctx = cc.egui_ctx.clone();
        for arg in std::env::args().skip(1) {
            if arg == "--run" {
                app.run_on_start = true;
            } else if arg == "--check-updates" {
                app.update_dialog = true;
            } else if !arg.starts_with("--") {
                app.open_file(&ctx, PathBuf::from(arg));
            }
        }
        app.check_only();
        app
    }

    // ------------------------------------------------------------------ helpers

    fn now(ctx: &egui::Context) -> f64 {
        ctx.input(|i| i.time)
    }

    /// The folder the running program may use for files (open, creat, unlink, ...): the
    /// folder of the .asm file, or Documents\ASM Test Bench for unsaved documents (examples,
    /// lessons, new ones).
    fn files_root(&self) -> PathBuf {
        if let Some(dir) = self.s.file.as_ref().and_then(|f| f.parent()) {
            return dir.to_path_buf();
        }
        let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        home.join("Documents").join("ASM Test Bench")
    }

    /// The document now matches what is on disk (or the example / new template it came from).
    fn mark_clean(&mut self) {
        self.s.dirty = false;
        self.s.saved_source = self.s.source.clone();
    }

    fn notify(&mut self, ctx: &egui::Context, msg: impl Into<String>, col: Color32) {
        self.toast = Some((msg.into(), col, Self::now(ctx)));
    }

    fn prog(&self) -> Option<&Rc<Program>> {
        self.dbg.as_ref().map(|d| &d.m.prog)
    }

    fn error_count(&self) -> usize {
        self.diags.iter().filter(|d| d.severity == Severity::Error).count()
    }

    fn stale(&self) -> bool {
        self.dbg.is_some() && self.session_src != self.s.source
    }

    /// File name of the document: the saved file, else the example it came from, else untitled.
    fn doc_name(&self) -> String {
        self.s.file.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).or_else(|| self.s.doc_name.clone()).unwrap_or_else(|| "untitled.asm".into())
    }

    fn title(&self) -> String {
        let name = self.doc_name();
        format!("{}{} — ASM Test Bench", if self.s.dirty { "● " } else { "" }, name)
    }

    /// Re-assembles for diagnostics only (keeps any session).
    fn check_only(&mut self) {
        let (prog, d) = asm::assemble(&self.s.source);
        // Autocomplete offers the program's labels / variables / constants as they are now.
        self.completions = super::complete::all_items(&prog.symbols);
        // The program size for the status bar (instructions, bytes of .data + .bss).
        self.prog_size = (prog.instrs.len(), prog.data.len() as u32 + prog.bss_size);
        self.instr_lines = prog.instrs.iter().map(|i| i.line).collect();
        self.diags = d;
    }

    /// Assembles and creates a new paused session at the entry point.
    fn start_session(&mut self, ctx: &egui::Context) -> bool {
        self.last_run = None;
        let (prog, diags) = asm::assemble(&self.s.source);
        self.diags = diags;
        let errors = self.error_count();
        if errors > 0 {
            self.dbg = None;
            self.s.bottom_tab = BottomTab::Problems;
            if let Some(d) = self.diags.iter().find(|d| d.severity == Severity::Error) {
                self.scroll_to = Some(d.line);
            }
            self.notify(ctx, format!("Assembly failed: {errors} error{}", if errors == 1 { "" } else { "s" }), RED);
            return false;
        }
        let entry_line = prog.instr_at(prog.entry).map(|i| i.line);
        let mut m = Machine::new(Rc::new(prog));
        m.files = crate::files::Sandbox::new(Some(self.files_root()));
        self.dbg = Some(Debugger::new(m));
        self.session_src = self.s.source.clone();
        self.screen_auto_opened = false;
        self.stop_msg = None;
        self.scroll_to = entry_line;
        true
    }

    /// Makes sure there is a usable session (starting one if needed). Returns false if
    /// assembly failed. `fresh` is set when a new session was created.
    fn ensure_session(&mut self, ctx: &egui::Context) -> Option<bool> {
        let restart = match &self.dbg {
            None => true,
            Some(d) => self.stale() || matches!(d.m.status, Status::Exited(_)),
        };
        if restart {
            return self.start_session(ctx).then_some(true);
        }
        Some(false)
    }

    /// F9 / menu: toggles a breakpoint at `line`, or at the next instruction line when
    /// `line` is a comment, label, blank or data line.
    fn toggle_bp(&mut self, line: usize) {
        if let Some(l) = editor::bp_target(&self.bps, Some(&self.instr_lines), line) {
            if !self.bps.remove(&l) {
                self.bps.insert(l);
            }
        }
    }

    fn bp_addrs(&self) -> HashSet<u32> {
        let Some(p) = self.prog() else { return HashSet::new() };
        self.bps.iter().filter_map(|l| p.line_addr.get(l).copied()).collect()
    }

    fn exec_line(&self) -> Option<usize> {
        let d = self.dbg.as_ref()?;
        d.m.current_instr().map(|i| i.line)
    }

    // ------------------------------------------------------------------ actions

    fn act_run(&mut self, ctx: &egui::Context) {
        let Some(fresh) = self.ensure_session(ctx) else { return };
        let d = self.dbg.as_mut().unwrap();
        if let Status::Fault(_) = d.m.status {
            self.notify(ctx, "The program stopped with an error. Step back or reset to continue.", ORANGE);
            return;
        }
        if fresh {
            d.start_fresh(Mode::Run);
        } else {
            d.start(Mode::Run);
        }
        self.budget = 0.0;
        self.stop_msg = None;
    }

    fn act_pause(&mut self) {
        if let Some(d) = &mut self.dbg {
            d.mode = Mode::Idle;
            self.scroll_to = self.exec_line();
            self.stop_msg = Some(("Paused".into(), YELLOW));
        }
    }

    fn act_stop(&mut self) {
        self.last_run = None;
        self.dbg = None;
        self.stop_msg = None;
    }

    fn act_reset(&mut self, ctx: &egui::Context) {
        if self.start_session(ctx) {
            self.stop_msg = Some(("Reset: paused at the entry point".into(), YELLOW));
        }
    }

    fn act_step_into(&mut self, ctx: &egui::Context) {
        match self.ensure_session(ctx) {
            None => return,
            Some(true) => {
                self.stop_msg = Some(("Paused at the entry point; step again to execute it".into(), YELLOW));
                return;
            }
            Some(false) => {}
        }
        let d = self.dbg.as_mut().unwrap();
        d.mode = Mode::Idle;
        d.m.step();
        self.after_manual_step(ctx);
    }

    fn act_step_over(&mut self, ctx: &egui::Context) {
        match self.ensure_session(ctx) {
            None => return,
            Some(true) => {
                self.stop_msg = Some(("Paused at the entry point; step again to execute it".into(), YELLOW));
                return;
            }
            Some(false) => {}
        }
        let d = self.dbg.as_mut().unwrap();
        match d.step_over_mode() {
            Some(mode) => {
                d.start(mode);
                self.budget = 0.0;
            }
            None => {
                d.m.step();
                self.after_manual_step(ctx);
            }
        }
    }

    fn act_step_out(&mut self, ctx: &egui::Context) {
        let Some(d) = self.dbg.as_mut() else { return };
        if d.m.frames.is_empty() {
            self.notify(ctx, "Not inside a called function (the call stack is empty).", ORANGE);
            return;
        }
        let mode = d.step_out_mode();
        d.start(mode);
        self.budget = 0.0;
    }

    fn act_step_back(&mut self, ctx: &egui::Context) {
        let Some(d) = self.dbg.as_mut() else { return };
        d.mode = Mode::Idle;
        if d.m.step_back() {
            self.stop_msg = Some((format!("Stepped back (step {})", d.m.steps), PURPLE));
            self.scroll_to = self.exec_line();
        } else {
            self.notify(ctx, "Nothing to step back to.", TEXT_DIM);
        }
    }

    fn act_run_to(&mut self, ctx: &egui::Context, line: usize) {
        let Some(fresh) = self.ensure_session(ctx) else { return };
        let d = self.dbg.as_mut().unwrap();
        // The first instruction at or after the line.
        let addr = d.m.prog.line_addr.iter().filter(|(l, _)| **l >= line).min_by_key(|(l, _)| **l).map(|(_, a)| *a);
        match addr {
            Some(a) => {
                if fresh {
                    d.start_fresh(Mode::RunTo(a));
                } else {
                    d.start(Mode::RunTo(a));
                }
                self.budget = 0.0;
            }
            None => self.notify(ctx, "No instruction at or after that line.", ORANGE),
        }
    }

    fn after_manual_step(&mut self, ctx: &egui::Context) {
        let Some(d) = &self.dbg else { return };
        let stop = match &d.m.status {
            Status::Exited(_) => Some(Stop::Exited),
            Status::Fault(_) => Some(Stop::Fault),
            Status::WaitInput => Some(Stop::WaitInput),
            Status::Ready => None,
        };
        match stop {
            Some(s) => self.handle_stop(ctx, s),
            None => {
                self.scroll_to = self.exec_line();
                self.stop_msg = None;
            }
        }
    }

    fn handle_stop(&mut self, ctx: &egui::Context, stop: Stop) {
        let exited = matches!(stop, Stop::Exited);
        let Some(d) = &self.dbg else { return };
        let line = d.m.current_instr().map(|i| i.line + 1).unwrap_or(0);
        self.scroll_to = self.exec_line();
        match stop {
            Stop::Breakpoint => self.stop_msg = Some((format!("Breakpoint hit at line {line}"), YELLOW)),
            Stop::Int3 => self.stop_msg = Some((format!("int3 at line {}", d.m.prog.instr_at(d.m.prev_eip).map(|i| i.line + 1).unwrap_or(0)), YELLOW)),
            Stop::Done => self.stop_msg = Some((format!("Paused at line {line}"), YELLOW)),
            Stop::Exited => {
                let code = if let Status::Exited(c) = d.m.status { c } else { 0 };
                let msg = format!("Program exited with code {code} after {} steps", d.m.steps);
                self.stop_msg = Some((msg.clone(), if code == 0 { GREEN } else { ORANGE }));
                self.s.bottom_tab = BottomTab::Console;
                self.notify(ctx, msg, if code == 0 { GREEN } else { ORANGE });
            }
            Stop::Fault => {
                let msg = if let Status::Fault(m) = &d.m.status { m.clone() } else { String::new() };
                self.stop_msg = Some((format!("Error at line {line}: {msg}"), RED));
                self.notify(ctx, format!("Line {line}: {msg}"), RED);
            }
            Stop::WaitInput => {
                self.stop_msg = Some(("Waiting for console input…".into(), ORANGE));
                self.s.bottom_tab = BottomTab::Console;
                self.focus_console = true;
            }
        }
        // A finished program ends the session by itself (like Stop). Its output and screen
        // stay visible until the next run.
        if exited {
            self.last_run = self.dbg.take();
        }
    }

    // ------------------------------------------------------------------ files

    /// Opens a file in a new tab (or switches to the tab that already has it).
    fn open_file(&mut self, ctx: &egui::Context, path: PathBuf) {
        if let Some(i) = (0..self.tabs.len()).find(|&i| self.tab_doc(i).0 == Some(&path)) {
            self.switch_to(ctx, i);
            return;
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let source = editor::tabify(&text.replace("\r\n", "\n"));
                self.s.recent.retain(|p| p != &path);
                self.s.recent.insert(0, path.clone());
                self.s.recent.truncate(8);
                self.open_tab(ctx, SavedDoc { saved_source: source.clone(), source, file: Some(path), ..Default::default() });
            }
            Err(e) => {
                // A file that vanished while the app was open: drop it from Recent too.
                if !path.exists() {
                    self.s.recent.retain(|p| p != &path);
                }
                self.toast = Some((format!("Can't open {}: {e}", path.display()), RED, 0.0));
            }
        }
    }

    /// Opens a built-in example as an unsaved document named after it, in a new tab (or
    /// switches to the tab that already shows it).
    fn open_example(&mut self, ctx: &egui::Context, file_name: &'static str) {
        if let Some(i) = (0..self.tabs.len()).find(|&i| self.tab_doc(i) == (None, Some(file_name))) {
            self.switch_to(ctx, i);
            return;
        }
        let Some(src) = examples::source(file_name) else { return };
        let source = editor::tabify(&src.replace("\r\n", "\n"));
        self.open_tab(ctx, SavedDoc { saved_source: source.clone(), source, doc_name: Some(file_name.to_string()), ..Default::default() });
    }

    // ------------------------------------------------------------------ tabs

    fn new_tab_id(&mut self) -> u64 {
        self.next_tab_id += 1;
        self.next_tab_id
    }

    fn active_index(&self) -> usize {
        self.tabs.iter().position(|t| t.parked.is_none()).unwrap_or(0)
    }

    /// (file, example name) of the document in tab `i`.
    fn tab_doc(&self, i: usize) -> (Option<&PathBuf>, Option<&str>) {
        match self.tabs.get(i).and_then(|t| t.parked.as_ref()) {
            Some(p) => (p.doc.file.as_ref(), p.doc.doc_name.as_deref()),
            None => (self.s.file.as_ref(), self.s.doc_name.as_deref()),
        }
    }

    /// Tab `i`: (name, unsaved, tooltip).
    fn tab_label(&self, i: usize) -> (String, bool, String) {
        let (file, example, dirty) = match self.tabs.get(i).and_then(|t| t.parked.as_ref()) {
            Some(p) => (p.doc.file.as_ref(), p.doc.doc_name.as_deref(), p.doc.dirty),
            None => (self.s.file.as_ref(), self.s.doc_name.as_deref(), self.s.dirty),
        };
        let name = file.and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).or_else(|| example.map(str::to_string)).unwrap_or_else(|| "untitled.asm".into());
        let tip = match (file, example) {
            (Some(p), _) => p.display().to_string(),
            (None, Some(_)) => "Built-in example: Save stores your own copy".into(),
            (None, None) => "New document (not saved yet)".into(),
        };
        (name, dirty, tip)
    }

    /// Moves the active document out of the App's fields. A running program is paused.
    fn park_active(&mut self, ctx: &egui::Context) -> Parked {
        if let Some(d) = &mut self.dbg {
            if d.running() {
                d.mode = Mode::Idle;
                self.stop_msg = Some(("Paused (switched to another tab)".into(), YELLOW));
            }
        }
        let editor_state = editor::detach(ctx, &mut self.s.source);
        let take = std::mem::take::<String>;
        Parked {
            doc: SavedDoc {
                source: take(&mut self.s.source),
                file: self.s.file.take(),
                doc_name: self.s.doc_name.take(),
                dirty: self.s.dirty,
                saved_source: take(&mut self.s.saved_source),
                breakpoints: Vec::new(),
                caret: self.cursor,
            },
            bps: std::mem::take(&mut self.bps),
            dbg: self.dbg.take(),
            last_run: self.last_run.take(),
            session_src: take(&mut self.session_src),
            stop_msg: self.stop_msg.take(),
            cursor: self.cursor,
            screen_auto_opened: self.screen_auto_opened,
            editor_state,
        }
    }

    /// Makes `p` the active document.
    fn unpark(&mut self, ctx: &egui::Context, p: Parked) {
        self.s.source = p.doc.source;
        self.s.file = p.doc.file;
        self.s.doc_name = p.doc.doc_name;
        self.s.dirty = p.doc.dirty;
        self.s.saved_source = p.doc.saved_source;
        self.bps = p.bps;
        self.dbg = p.dbg;
        self.last_run = p.last_run;
        self.session_src = p.session_src;
        self.stop_msg = p.stop_msg;
        self.cursor = p.cursor;
        self.screen_auto_opened = p.screen_auto_opened;
        editor::attach(ctx, p.editor_state);
        self.check_only(); // diagnostics and autocomplete for this document
        self.scroll_to = Some(self.cursor.0);
    }

    fn touch_mru(&mut self, id: u64) {
        self.mru.retain(|&x| x != id);
        self.mru.insert(0, id);
    }

    /// Shows tab `i`.
    fn switch_to(&mut self, ctx: &egui::Context, i: usize) {
        let cur = self.active_index();
        if i == cur || i >= self.tabs.len() {
            return;
        }
        let Some(target) = self.tabs[i].parked.take() else { return };
        let p = self.park_active(ctx);
        self.tabs[cur].parked = Some(p);
        self.unpark(ctx, target);
        if self.tab_switch.is_none() {
            self.touch_mru(self.tabs[i].id);
        }
    }

    /// The active tab is an untouched new document (opening something can just replace it).
    fn active_is_blank(&self) -> bool {
        self.s.file.is_none() && self.s.doc_name.is_none() && !self.s.dirty && self.s.source == NEW_TEMPLATE && self.dbg.is_none()
    }

    /// Shows `doc` in a new tab at the right end of the tab bar. An untouched new document in
    /// the current tab is closed instead of being kept next to it.
    fn open_tab(&mut self, ctx: &egui::Context, doc: SavedDoc) {
        let cur = self.active_index();
        if self.active_is_blank() {
            drop(self.park_active(ctx));
            let id = self.tabs.remove(cur).id;
            self.mru.retain(|&x| x != id);
        } else {
            let p = self.park_active(ctx);
            self.tabs[cur].parked = Some(p);
        }
        let id = self.new_tab_id();
        self.tabs.push(Tab { id, parked: None });
        self.unpark(ctx, Parked::from_doc(doc));
        self.scroll_to = Some(0);
        self.touch_mru(self.tabs[self.active_index()].id);
    }

    /// Asks to close tab `i` (switching to it first, so unsaved changes can be saved).
    fn close_tab(&mut self, ctx: &egui::Context, i: usize) {
        self.switch_to(ctx, i);
        self.request(ctx, Pending::CloseTab);
    }

    /// Closes the active tab (its unsaved changes are dropped) and shows the most recently
    /// used other one. The last tab becomes a new empty document instead.
    fn close_active(&mut self, ctx: &egui::Context) {
        let cur = self.active_index();
        let id = self.tabs[cur].id;
        drop(self.park_active(ctx));
        self.mru.retain(|&x| x != id);
        if self.tabs.len() == 1 {
            self.unpark(ctx, Parked::from_doc(new_doc()));
            self.mru = vec![id];
            return;
        }
        let next = self.mru.first().and_then(|n| self.tabs.iter().position(|t| t.id == *n)).unwrap_or(if cur > 0 { cur - 1 } else { 1 });
        let target = self.tabs[next].parked.take().expect("other tabs are parked");
        self.tabs.remove(cur);
        self.unpark(ctx, target);
        self.touch_mru(self.tabs[self.active_index()].id);
    }

    /// Ctrl+Tab / Ctrl+Shift+Tab: the previously used tab, and further back while Ctrl is
    /// held (stops at the oldest, doesn't wrap around); the order of use is updated when Ctrl
    /// is released.
    fn tab_switch_key(&mut self, ctx: &egui::Context, back: bool) {
        if self.tabs.len() < 2 {
            return;
        }
        let (order, mut pos) = self.tab_switch.take().unwrap_or_else(|| (self.mru.clone(), 0));
        pos = if back { pos.saturating_sub(1) } else { (pos + 1).min(order.len() - 1) };
        let target = order[pos];
        self.tab_switch = Some((order, pos));
        if let Some(i) = self.tabs.iter().position(|t| t.id == target) {
            self.switch_to(ctx, i);
        }
    }

    /// Ctrl released after Ctrl+Tab: the tab we ended on is now the most recently used.
    fn end_tab_switch(&mut self) {
        self.tab_switch = None;
        let id = self.tabs[self.active_index()].id;
        self.touch_mru(id);
    }

    fn save(&mut self, ctx: &egui::Context, save_as: bool) {
        // Unsaved documents (new, or a built-in example) always ask where to save.
        let path = match (&self.s.file, save_as) {
            (Some(p), false) => Some(p.clone()),
            (Some(p), true) => filedlg::save(Some(p)),
            (None, _) => filedlg::save(Some(std::path::Path::new(&self.doc_name()))),
        };
        let Some(mut path) = path else { return };
        let mut result = std::fs::write(&path, self.s.source.replace('\n', "\r\n"));
        // The file's folder may be gone (moved / renamed project, removed drive): offer Save As,
        // and the file picked there becomes the document's file from now on.
        if let Err(e) = &result {
            let name = path.file_name().map(std::path::PathBuf::from);
            // Start in the old folder if it still exists, otherwise just suggest the name.
            let initial = if path.parent().is_some_and(|d| d.is_dir()) { Some(path.clone()) } else { name };
            let title = format!("Couldn't save to {} ({e}). Save as", path.display());
            match filedlg::save_titled(initial.as_deref(), &title) {
                Some(p) => {
                    path = p;
                    result = std::fs::write(&path, self.s.source.replace('\n', "\r\n"));
                }
                None => {
                    self.notify(ctx, format!("Not saved: can't write {} ({e})", path.display()), RED);
                    return;
                }
            }
        }
        match result {
            Ok(()) => {
                self.mark_clean();
                self.s.recent.retain(|p| p != &path);
                self.s.recent.insert(0, path.clone());
                self.s.recent.truncate(8);
                self.s.file = Some(path.clone());
                self.s.doc_name = None;
                self.notify(ctx, format!("Saved {}", path.display()), GREEN);
            }
            Err(e) => self.notify(ctx, format!("Can't save: {e}"), RED),
        }
    }

    /// Runs `action`; closing a tab with unsaved changes asks first. (New / Open open a new
    /// tab, so they never lose anything.)
    fn request(&mut self, ctx: &egui::Context, action: Pending) {
        if matches!(action, Pending::CloseTab) && self.s.dirty {
            self.confirm = Some(action);
        } else {
            self.do_pending(ctx, action);
        }
    }

    fn do_pending(&mut self, ctx: &egui::Context, action: Pending) {
        match action {
            Pending::New => self.open_tab(ctx, new_doc()),
            Pending::Open => {
                if let Some(p) = filedlg::open(self.s.file.as_deref()) {
                    self.open_file(ctx, p);
                }
            }
            Pending::OpenPath(p) => self.open_file(ctx, p),
            Pending::OpenExample(name) => self.open_example(ctx, name),
            Pending::CloseTab => self.close_active(ctx),
        }
    }

    /// Keeps breakpoints on the same code when lines are inserted or removed.
    fn shift_breakpoints(&mut self, old: &str, new: &str) {
        let old_lines = old.split('\n').count() as isize;
        let new_lines = new.split('\n').count() as isize;
        let delta = new_lines - old_lines;
        if delta == 0 || self.bps.is_empty() {
            return;
        }
        let common = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
        let change_line = old[..common].matches('\n').count();
        // A change at the very start of a line moves that line too.
        let at_line_start = common == 0 || old.as_bytes()[common - 1] == b'\n';
        let first_moved = if at_line_start { change_line } else { change_line + 1 };
        let mut out = BTreeSet::new();
        for &b in &self.bps {
            if b < first_moved {
                out.insert(b);
            } else if delta < 0 && (b as isize) < first_moved as isize - delta {
                // Line deleted.
                if at_line_start && b == first_moved && (b as isize + delta) >= 0 {
                    continue;
                }
                continue;
            } else {
                out.insert((b as isize + delta).max(0) as usize);
            }
        }
        self.bps = out;
    }

    // ------------------------------------------------------------------ shortcuts

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let k = |m: Modifiers, key: Key| ctx.input_mut(|i| i.consume_key(m, key));
        let shift = Modifiers::SHIFT;
        let ctrl = Modifiers::COMMAND;
        let none = Modifiers::NONE;
        // Tabs: Ctrl+Tab = previously used tab (further back while Ctrl is held), Ctrl+Shift+Tab
        // = the other way; Ctrl+W / Ctrl+F4 close the tab.
        if k(ctrl | shift, Key::Tab) {
            self.tab_switch_key(ctx, true);
        } else if k(ctrl, Key::Tab) {
            self.tab_switch_key(ctx, false);
        }
        if self.tab_switch.is_some() && !ctx.input(|i| i.modifiers.command) {
            self.end_tab_switch();
        }
        if k(ctrl, Key::W) || k(ctrl, Key::F4) {
            let i = self.active_index();
            self.close_tab(ctx, i);
        }
        if k(ctrl | shift, Key::F5) {
            self.act_reset(ctx);
        } else if k(shift, Key::F5) {
            self.act_stop();
        } else if k(none, Key::F5) {
            self.act_run(ctx);
        }
        if k(none, Key::F6) {
            self.act_pause();
        }
        if k(none, Key::F7) || k(ctrl, Key::B) {
            if self.start_session(ctx) {
                self.notify(ctx, "Assembled successfully. Paused at the entry point.", GREEN);
            }
        }
        if k(shift, Key::F11) {
            self.act_step_out(ctx);
        } else if k(none, Key::F11) {
            self.act_step_into(ctx);
        }
        if k(shift, Key::F10) {
            self.act_step_back(ctx);
        } else if k(none, Key::F10) {
            self.act_step_over(ctx);
        }
        if k(none, Key::F9) {
            self.toggle_bp(self.cursor.0);
        }
        if k(ctrl, Key::S) {
            self.save(ctx, false);
        }
        if k(ctrl | shift, Key::S) {
            self.save(ctx, true);
        }
        if k(ctrl, Key::O) {
            self.request(ctx, Pending::Open);
        }
        if k(ctrl, Key::N) {
            self.request(ctx, Pending::New);
        }
        if k(ctrl, Key::Plus) || k(ctrl, Key::Equals) {
            self.s.code_size = (self.s.code_size + 1.0).min(32.0);
        }
        if k(ctrl, Key::Minus) {
            self.s.code_size = (self.s.code_size - 1.0).max(9.0);
        }
    }

    // ------------------------------------------------------------------ running

    fn tick(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f64().min(0.25);
        self.last_frame = now;
        let bps = self.bp_addrs();
        let speed = self.s.speed;
        let Some(d) = self.dbg.as_mut() else { return };
        if !d.running() {
            return;
        }
        if d.m.status == Status::WaitInput && d.m.input.iter().all(|&b| b != b'\n') {
            return; // still waiting
        }
        let (max_steps, deadline) = if speed >= MAX_SPEED {
            (u64::MAX, now + Duration::from_millis(14))
        } else {
            self.budget += dt * speed as f64;
            let n = self.budget.floor();
            self.budget -= n;
            (n as u64, now + Duration::from_millis(14))
        };
        if max_steps > 0 {
            let stop = d.run_slice(&bps, max_steps, deadline);
            if let Some(s) = stop {
                self.handle_stop(ctx, s);
            } else if speed < MAX_SPEED {
                // Animated: follow the execution in the editor.
                self.scroll_to = self.exec_line();
            }
        }
        ctx.request_repaint();
    }
}

// ====================================================================== drawing

enum TabClick {
    None,
    Select,
    Close,
}

/// One document tab: the name, then an unsaved-changes dot or a close button (shown on the
/// active / hovered tab; the dot turns into the button under the pointer). Middle-click closes.
fn doc_tab(ui: &mut Ui, name: &str, dirty: bool, active: bool, tip: &str) -> TabClick {
    let font = egui::FontId::proportional(13.0);
    let g = ui.fonts_mut(|f| f.layout_no_wrap(name.to_string(), font, Color32::WHITE));
    let (pad, close_w) = (12.0, 18.0);
    let size = egui::vec2(pad + g.size().x + 8.0 + close_w + 6.0, 30.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let resp = resp.on_hover_text(tip);
    let close_rect = egui::Rect::from_center_size(egui::pos2(rect.right() - 6.0 - close_w / 2.0, rect.center().y), egui::vec2(close_w, close_w));
    let close = ui.interact(close_rect, resp.id.with("close"), Sense::click());
    let p = ui.painter();
    let hovered = resp.hovered() || close.hovered();
    let bg = if active { BG } else if hovered { CARD } else { BAR };
    p.rect_filled(rect, egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, bg);
    if active {
        p.line_segment([rect.left_top() + egui::vec2(6.0, 1.0), rect.right_top() + egui::vec2(-6.0, 1.0)], Stroke::new(2.0, ACCENT_HI));
    }
    p.galley(egui::pos2(rect.left() + pad, rect.center().y - g.size().y / 2.0), g, if active { TEXT } else { TEXT_DIM });
    let c = close_rect.center();
    if dirty && !close.hovered() {
        p.circle_filled(c, 4.0, ORANGE);
    } else if active || hovered {
        if close.hovered() {
            p.rect_filled(close_rect, 4.0, CARD_HI);
        }
        let col = if close.hovered() { TEXT } else { TEXT_DIM };
        let d = 4.0;
        p.line_segment([c + egui::vec2(-d, -d), c + egui::vec2(d, d)], Stroke::new(1.4, col));
        p.line_segment([c + egui::vec2(-d, d), c + egui::vec2(d, -d)], Stroke::new(1.4, col));
    }
    if close.clicked() || resp.middle_clicked() {
        TabClick::Close
    } else if resp.clicked() {
        TabClick::Select
    } else {
        TabClick::None
    }
}

/// Same text, ignoring spaces at the ends of lines.
fn same_text(a: &str, b: &str) -> bool {
    a.split('\n').map(|l| l.trim_end_matches(' ')).eq(b.split('\n').map(|l| l.trim_end_matches(' ')))
}

fn fmt_num(v: u32, fmt: NumFmt, size: Size) -> String {
    let v = v & size.mask();
    match fmt {
        NumFmt::Hex => format!("0x{:0w$X}", v, w = (size.bytes() * 2) as usize),
        NumFmt::Dec => format!("{v}"),
        NumFmt::Signed => format!("{}", size.sext(v) as i32),
        NumFmt::Bin => {
            // Groups of 8 bits: 00000000 00000000 00000000 00000100
            let b = format!("{:0w$b}", v, w = size.bits() as usize);
            b.as_bytes().chunks(8).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join(" ")
        }
    }
}

fn number_tooltip(ui: &mut Ui, v: i64) {
    let u = v as u32;
    egui::Grid::new("num_tip").num_columns(2).spacing([12.0, 2.0]).show(ui, |ui| {
        ui.label(RichText::new("Decimal").color(TEXT_DIM));
        ui.label(RichText::new(format!("{}", u as i32)).family(code()));
        ui.end_row();
        if (u as i32) < 0 {
            ui.label(RichText::new("Unsigned").color(TEXT_DIM));
            ui.label(RichText::new(format!("{u}")).family(code()));
            ui.end_row();
        }
        ui.label(RichText::new("Hex").color(TEXT_DIM));
        ui.label(RichText::new(format!("0x{u:X}")).family(code()));
        ui.end_row();
        ui.label(RichText::new("Binary").color(TEXT_DIM));
        let bits = if u <= 0xFF { 8 } else if u <= 0xFFFF { 16 } else { 32 };
        let b = format!("{:0w$b}", u, w = bits);
        let grouped: Vec<String> = b.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).into_owned()).collect();
        ui.label(RichText::new(grouped.join(" ")).family(code()));
        ui.end_row();
        if (32..127).contains(&u) {
            ui.label(RichText::new("Character").color(TEXT_DIM));
            ui.label(RichText::new(format!("'{}'", u as u8 as char)).family(code()));
            ui.end_row();
        }
    });
}

/// Hex / unsigned / signed / binary (/ char) of a register value, at the register's size.
fn register_value_grid(ui: &mut Ui, v: u32, size: Size) {
    let v = v & size.mask();
    egui::Grid::new("reg_tip").num_columns(2).spacing([12.0, 2.0]).show(ui, |ui| {
        let row = |ui: &mut Ui, k: &str, val: String| {
            ui.label(RichText::new(k).color(TEXT_DIM));
            ui.label(RichText::new(val).family(code()));
            ui.end_row();
        };
        row(ui, "Hex", format!("0x{:0w$X}", v, w = (size.bytes() * 2) as usize));
        row(ui, "Unsigned", format!("{v}"));
        row(ui, "Signed", format!("{}", size.sext(v) as i32));
        let b = format!("{:0w$b}", v, w = size.bits() as usize);
        let grouped: Vec<String> = b.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).into_owned()).collect();
        row(ui, "Binary", grouped.join(" "));
        let low = (v & 0xFF) as u8;
        if (32..127).contains(&low) && (size == Size::B || v < 0x100) {
            row(ui, "Character", format!("'{}'", low as char));
        }
    });
}

fn instr_doc_ui(ui: &mut Ui, d: &docs::Doc) {
    ui.set_max_width(440.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(d.name).family(bold()).size(15.0).color(SYN_MNEMONIC));
        ui.label(RichText::new(d.group).small().color(TEXT_DIM));
    });
    ui.label(RichText::new(d.syntax).family(code()).color(SYN_LABEL));
    ui.add_space(2.0);
    ui.label(d.summary);
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Flags:").color(TEXT_DIM));
        ui.label(RichText::new(d.flags).family(code()));
    });
    egui::Frame::new().fill(BG).corner_radius(4.0).inner_margin(6.0).show(ui, |ui| {
        ui.label(RichText::new(d.example).family(code()).color(SYN_TEXT));
    });
}

impl App {
    fn toolbar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let has = self.dbg.is_some();
        let running = self.dbg.as_ref().is_some_and(|d| d.running());
        let faulted = self.dbg.as_ref().is_some_and(|d| matches!(d.m.status, Status::Fault(_)));
        let paused_session = has && !running;

        ui.horizontal_centered(|ui| {
            // Logo.
            let (r, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), Sense::hover());
            match &self.logo {
                Some(tex) => {
                    ui.painter().image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                }
                None => {
                    ui.painter().rect_filled(r, 6.0, ACCENT);
                    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "A", egui::FontId::new(15.0, bold()), Color32::WHITE);
                }
            }
            ui.label(RichText::new("ASM Test Bench").family(bold()).size(15.0).color(TEXT));
            ui.add_space(6.0);

            // Menu buttons styled flat, inside a MenuBar so that while one menu is open,
            // hovering another opens it. MenuBar fills the available width, so it gets a box
            // exactly as wide as its buttons (button text + padding, plus the gaps).
            let menus_w = {
                let pad = ui.spacing().button_padding.x;
                let gap = ui.spacing().item_spacing.x;
                let font = egui::TextStyle::Button.resolve(ui.style());
                let names = ["File", "Debug", "View", "Help"];
                let text_w: f32 = ui.fonts_mut(|f| names.iter().map(|n| f.layout_no_wrap(n.to_string(), font.clone(), Color32::WHITE).size().x).sum());
                text_w + names.len() as f32 * 2.0 * pad + (names.len() - 1) as f32 * gap + 1.0
            };
            let bar_size = egui::vec2(menus_w, ui.spacing().interact_size.y);
            ui.allocate_ui_with_layout(bar_size, egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let v = &mut ui.visuals_mut().widgets;
                v.inactive.weak_bg_fill = Color32::TRANSPARENT;
                v.inactive.bg_stroke = Stroke::NONE;
                v.hovered.bg_stroke = Stroke::NONE;
                v.open.bg_stroke = Stroke::NONE;
                // No-op style: keep the title bar's own button look instead of MenuBar's.
                egui::MenuBar::new().style(|_: &mut egui::Style| {}).ui(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("New                    Ctrl+N").clicked() {
                            self.request(&ctx, Pending::New);
                            ui.close();
                        }
                        if ui.button("Open…                Ctrl+O").clicked() {
                            ui.close();
                            self.request(&ctx, Pending::Open);
                        }
                        ui.menu_button("Examples", |ui| {
                            let list: Vec<(String, &'static str)> = self.example_list.iter().map(|e| (e.title.clone(), e.file_name)).collect();
                            for (title, file_name) in list {
                                if ui.button(title).on_hover_text(format!("{file_name} (built in; Save stores a copy)")).clicked() {
                                    ui.close();
                                    self.request(&ctx, Pending::OpenExample(file_name));
                                }
                            }
                        });
                        ui.menu_button("Tutorial", |ui| {
                            for (title, file_name) in self.tutorial_list.iter().map(|e| (e.title.clone(), e.file_name)).collect::<Vec<_>>() {
                                if ui.button(title).on_hover_text("A lesson: read the comments, step through it with F10, then try the exercises at the end").clicked() {
                                    ui.close();
                                    self.request(&ctx, Pending::OpenExample(file_name));
                                }
                            }
                        });
                        ui.menu_button("Advanced Tutorial", |ui| {
                            for (title, file_name) in self.advanced_list.iter().map(|e| (e.title.clone(), e.file_name)).collect::<Vec<_>>() {
                                if ui.button(title).on_hover_text("An advanced lesson (after the Tutorial): a real algorithm, explained step by step").clicked() {
                                    ui.close();
                                    self.request(&ctx, Pending::OpenExample(file_name));
                                }
                            }
                        });
                        if !self.s.recent.is_empty() {
                            ui.menu_button("Recent", |ui| {
                                for p in self.s.recent.clone() {
                                    if ui.button(p.display().to_string()).clicked() {
                                        ui.close();
                                        self.request(&ctx, Pending::OpenPath(p));
                                    }
                                }
                            });
                        }
                        ui.separator();
                        if ui.button("Save                   Ctrl+S").clicked() {
                            ui.close();
                            self.save(&ctx, false);
                        }
                        if ui.button("Save As…       Ctrl+Shift+S").clicked() {
                            ui.close();
                            self.save(&ctx, true);
                        }
                        let files_dir = self.files_root();
                        if ui.button("Open Files Folder").on_hover_text(format!("Where this program's files go (open / creat / unlink system calls):\n{}", files_dir.display())).clicked() {
                            ui.close();
                            let _ = std::fs::create_dir_all(&files_dir);
                            let _ = std::process::Command::new("explorer").arg(&files_dir).spawn();
                        }
                        if ui.button("Close Tab            Ctrl+W").clicked() {
                            ui.close();
                            let i = self.active_index();
                            self.close_tab(&ctx, i);
                        }
                        ui.separator();
                        if ui.button("Exit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.menu_button("Debug", |ui| {
                        let items: [(&str, &str); 9] = [
                            ("Debug (start, paused)", "F7"),
                            ("Run / Continue", "F5"),
                            ("Pause", "F6"),
                            ("Stop", "Shift+F5"),
                            ("Reset to entry", "Ctrl+Shift+F5"),
                            ("Step Into", "F11"),
                            ("Step Over", "F10"),
                            ("Step Out", "Shift+F11"),
                            ("Step Back", "Shift+F10"),
                        ];
                        for (i, (name, key)) in items.iter().enumerate() {
                            let b = ui.add(egui::Button::new(*name).shortcut_text(*key));
                            if b.clicked() {
                                ui.close();
                                match i {
                                    0 => {
                                        if self.start_session(&ctx) {
                                            self.notify(&ctx, "Assembled successfully. Paused at the entry point.", GREEN);
                                        }
                                    }
                                    1 => self.act_run(&ctx),
                                    2 => self.act_pause(),
                                    3 => self.act_stop(),
                                    4 => self.act_reset(&ctx),
                                    5 => self.act_step_into(&ctx),
                                    6 => self.act_step_over(&ctx),
                                    7 => self.act_step_out(&ctx),
                                    _ => self.act_step_back(&ctx),
                                }
                            }
                        }
                        ui.separator();
                        if ui.add(egui::Button::new("Toggle Breakpoint").shortcut_text("F9")).clicked() {
                            self.toggle_bp(self.cursor.0);
                            ui.close();
                        }
                        if ui.button("Remove All Breakpoints").clicked() {
                            self.bps.clear();
                            ui.close();
                        }
                    });
                    // View holds settings (checkboxes, sliders): it stays open while you change
                    // them and closes on a click outside it (or Esc).
                    let stay_open = egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
                    egui::containers::menu::MenuButton::new("View").config(stay_open).ui(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Code size");
                            ui.add(egui::Slider::new(&mut self.s.code_size, 9.0..=32.0).step_by(1.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Interface size");
                            ui.add(egui::Slider::new(&mut self.s.ui_size, 11.0..=20.0).step_by(0.5));
                        });
                        ui.menu_button("Code font", |ui| {
                            for &(f, installed) in code_fonts() {
                                let label = if f == BUILTIN_FONT { "Built-in (Hack)" } else { f };
                                let r = ui.add_enabled(installed, egui::Button::selectable(self.s.code_font == f, label));
                                if r.clicked() {
                                    self.s.code_font = f.to_string();
                                    ui.close();
                                }
                                if !installed {
                                    r.on_disabled_hover_text("Not installed on this computer");
                                }
                            }
                        });
                        ui.separator();
                        ui.checkbox(&mut self.s.show_hits, "Show execution counts in the editor");
                        ui.checkbox(&mut self.s.screen_open, "Screen window (128×128 graphics)");
                        ui.separator();
                        ui.checkbox(&mut self.s.align_operands, "Align operands: Space after an instruction pads to a column")
                            .on_hover_text("Typing \"mov\" + Space jumps to the operand column, so code lines up like the examples.");
                        ui.add_enabled_ui(self.s.align_operands, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Operand column");
                                ui.add(egui::Slider::new(&mut self.s.operand_column, 8..=24).step_by(4.0));
                            });
                        });
                        ui.checkbox(&mut self.s.italic_comments, "Italic comments");
                        ui.checkbox(&mut self.s.autocomplete, "Autocomplete: suggest instructions, registers and names while typing")
                            .on_hover_text("A list opens under the word you type. Up / Down choose, Enter or Tab inserts, Esc closes. Ctrl+Space opens it any time.");
                        ui.checkbox(&mut self.s.virtual_space, "Virtual space: the cursor can go past the end of a line")
                            .on_hover_text("Click anywhere to the right of the text, or use Up/Down/Right, and the cursor stays in that column. Spaces are only added if you type there.");
                        ui.checkbox(&mut self.s.align_comments, "Align comments: ';' after code moves to a column")
                            .on_hover_text("Typing ';' after an instruction moves the comment to the comment column (when the line is short enough), like the examples.");
                        ui.add_enabled_ui(self.s.align_comments, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Comment column");
                                ui.add(egui::Slider::new(&mut self.s.comment_column, 16..=64).step_by(4.0));
                            });
                        });
                        ui.separator();
                        ui.label(RichText::new("Number format").color(TEXT_DIM));
                        ui.radio_value(&mut self.s.num_fmt, NumFmt::Hex, "Hexadecimal");
                        ui.radio_value(&mut self.s.num_fmt, NumFmt::Dec, "Unsigned decimal");
                        ui.radio_value(&mut self.s.num_fmt, NumFmt::Signed, "Signed decimal");
                        ui.radio_value(&mut self.s.num_fmt, NumFmt::Bin, "Binary");
                    });
                    ui.menu_button("Help", |ui| {
                        if ui.button("Start the tutorial (lesson 1)").on_hover_text("All lessons: File > Tutorial").clicked() {
                            ui.close();
                            if let Some(first) = self.tutorial_list.first().map(|l| l.file_name) {
                                self.request(&ctx, Pending::OpenExample(first));
                            }
                        }
                        if ui.button("Instruction reference").clicked() {
                            self.s.bottom_tab = BottomTab::Reference;
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Check for updates…").clicked() {
                            ui.close();
                            self.open_update_dialog(&ctx);
                        }
                        ui.checkbox(&mut self.s.auto_update, "Check for updates automatically")
                            .on_hover_text("At start-up and every few hours; a new version downloads in the background and installs when ASM Test Bench restarts.");
                        ui.separator();
                        if ui.button("About ASM Test Bench").clicked() {
                            self.about = true;
                            ui.close();
                        }
                    });
                });
            });

            ui.add_space(10.0);
            let sep = |ui: &mut Ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(9.0, 22.0), Sense::hover());
                ui.painter().line_segment([r.center_top(), r.center_bottom()], Stroke::new(1.0, BORDER));
            };
            sep(ui);

            if widgets::primary_button(ui, Icon::Build, "Debug", Color32::from_rgb(0, 110, 200), !running, "Debug (F7)\nChecks the code and starts debugging, paused at the first instruction: then step with F10 / F11").clicked() && self.start_session(&ctx) {
                self.notify(&ctx, "Assembled successfully. Paused at the entry point.", GREEN);
            }
            ui.add_space(4.0);
            if running {
                if widgets::primary_button(ui, Icon::Pause, "Pause", Color32::from_rgb(190, 130, 20), true, "Pause (F6)").clicked() {
                    self.act_pause();
                }
            } else {
                let label = if paused_session && !self.stale() && !matches!(self.dbg.as_ref().map(|d| &d.m.status), Some(Status::Exited(_))) { "Continue" } else { "Run" };
                if widgets::primary_button(ui, Icon::Run, label, Color32::from_rgb(30, 140, 70), !faulted, "Run / Continue (F5)").clicked() {
                    self.act_run(&ctx);
                }
            }
            ui.add_space(4.0);
            if widgets::icon_button(ui, Icon::Stop, RED, has, "Stop debugging (Shift+F5)").clicked() {
                self.act_stop();
            }
            if widgets::icon_button(ui, Icon::Reset, ACCENT_HI, true, "Reset to the entry point (Ctrl+Shift+F5)").clicked() {
                self.act_reset(&ctx);
            }
            sep(ui);
            if widgets::icon_button(ui, Icon::StepInto, LINK, !running && !faulted, "Step Into (F11)\nExecute one instruction, entering calls").clicked() {
                self.act_step_into(&ctx);
            }
            if widgets::icon_button(ui, Icon::StepOver, LINK, !running && !faulted, "Step Over (F10)\nExecute one instruction; run calls to completion").clicked() {
                self.act_step_over(&ctx);
            }
            let can_out = paused_session && !faulted && self.dbg.as_ref().is_some_and(|d| !d.m.frames.is_empty());
            if widgets::icon_button(ui, Icon::StepOut, LINK, can_out, "Step Out (Shift+F11)\nRun until the current function returns").clicked() {
                self.act_step_out(&ctx);
            }
            let can_back = !running && self.dbg.as_ref().is_some_and(|d| d.m.can_step_back());
            if widgets::icon_button(ui, Icon::StepBack, PURPLE, can_back, "Step Back (Shift+F10)\nUndo the last instruction").clicked() {
                self.act_step_back(&ctx);
            }
            sep(ui);
            ui.label(RichText::new("Speed").color(TEXT_DIM));
            let label = if self.s.speed >= MAX_SPEED { "Max".to_string() } else { format!("{:.0}/s", self.s.speed) };
            ui.add(egui::Slider::new(&mut self.s.speed, 1.0..=MAX_SPEED).logarithmic(true).show_value(false))
                .on_hover_text("Instructions per second while running.\nSlow speeds animate execution line by line; far right = full speed.");
            ui.label(RichText::new(label).family(code()).color(TEXT));

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (text, fg, bg) = match &self.dbg {
                    None => ("EDITING", TEXT_DIM, Color32::from_rgb(48, 50, 58)),
                    Some(d) if d.running() && d.m.status == Status::WaitInput => ("WAITING FOR INPUT", Color32::BLACK, ORANGE),
                    Some(d) if d.running() => ("RUNNING", Color32::WHITE, ACCENT),
                    Some(d) => match d.m.status {
                        Status::Exited(_) => ("EXITED", Color32::BLACK, GREEN),
                        Status::Fault(_) => ("ERROR", Color32::WHITE, Color32::from_rgb(190, 50, 50)),
                        Status::WaitInput => ("WAITING FOR INPUT", Color32::BLACK, ORANGE),
                        Status::Ready => ("PAUSED", Color32::BLACK, YELLOW),
                    },
                };
                widgets::pill(ui, text, fg, bg);
            });
        });
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal_centered(|ui| {
            let (dot, msg) = match (&self.stop_msg, &self.dbg) {
                (Some((m, c)), _) => (*c, m.clone()),
                (None, Some(d)) if d.running() => (ACCENT_HI, "Running…".to_string()),
                (None, Some(_)) => (YELLOW, format!("Paused at line {}", self.exec_line().map(|l| l + 1).unwrap_or(0))),
                (None, None) => {
                    let e = self.error_count();
                    if e > 0 {
                        (RED, format!("{e} problem{} — see the Problems tab", if e == 1 { "" } else { "s" }))
                    } else {
                        (GREEN, "Ready — F5 to run, F10 to step".to_string())
                    }
                }
            };
            let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 4.0, dot);
            ui.label(RichText::new(msg).color(TEXT).size(12.5));
            if self.stale() {
                ui.label(RichText::new("  •  code changed since start — Reset (Ctrl+Shift+F5) to apply").color(ORANGE).size(12.5));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                self.update_notice(ui);
                ui.label(RichText::new(format!("Ln {}, Col {}", self.cursor.0 + 1, editor::visual_col_at(&self.s.source, self.cursor.0, self.cursor.1) + 1)).color(TEXT_DIM).size(12.5));
                if let Some(d) = &self.dbg {
                    ui.label(RichText::new(format!("Steps {}   ", d.m.steps)).color(TEXT_DIM).size(12.5));
                }
                // The program's size, always: the code as it is now (updated while typing).
                let (n, bytes) = self.prog_size;
                let plural = |k: usize, word: &str| format!("{k} {word}{}", if k == 1 { "" } else { "s" });
                ui.label(RichText::new(format!("{}, {} of data   ", plural(n, "instruction"), plural(bytes as usize, "byte"))).color(TEXT_DIM).size(12.5))
                    .on_hover_text("Size of the program: instructions in .text, bytes in .data and .bss");
            });
        });
    }

    fn editor_area(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        // Document tabs, and the problem count of the active one on the right.
        egui::Frame::new().fill(BAR).inner_margin(egui::Margin { left: 6, right: 8, top: 4, bottom: 0 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let mut clicked: Option<(usize, bool)> = None;
                let tabs_w = (ui.available_width() - 170.0).max(120.0);
                egui::ScrollArea::horizontal()
                    .id_salt("doc_tabs")
                    .max_width(tabs_w)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            for i in 0..self.tabs.len() {
                                let (name, dirty, tip) = self.tab_label(i);
                                let active = self.tabs[i].parked.is_none();
                                match doc_tab(ui, &name, dirty, active, &tip) {
                                    TabClick::Select => clicked = Some((i, false)),
                                    TabClick::Close => clicked = Some((i, true)),
                                    TabClick::None => {}
                                }
                            }
                        });
                    });
                if let Some((i, close)) = clicked {
                    if close {
                        self.close_tab(&ctx, i);
                    } else {
                        self.switch_to(&ctx, i);
                    }
                    // Give the editor the focus back next frame: this frame the editor still
                    // sees the click outside it and drops the focus.
                    self.focus_editor_on_start = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let e = self.error_count();
                    let w = self.diags.len() - e;
                    if e > 0 && widgets::pill(ui, &format!("{e} error{}", if e == 1 { "" } else { "s" }), Color32::WHITE, Color32::from_rgb(170, 45, 45)).interact(Sense::click()).clicked() {
                        self.s.bottom_tab = BottomTab::Problems;
                    }
                    if w > 0 {
                        widgets::pill(ui, &format!("{w} warning{}", if w == 1 { "" } else { "s" }), Color32::BLACK, YELLOW);
                    }
                    if e == 0 && w == 0 {
                        widgets::pill(ui, "✓ no problems", GREEN, Color32::from_rgb(30, 52, 38));
                    }
                });
            });
        });

        let (exec, prev) = match &self.dbg {
            Some(d) if !self.stale() => {
                let kind = if matches!(d.m.status, Status::Fault(_)) { ExecKind::Fault } else { ExecKind::Paused };
                let exec = if d.running() && self.s.speed >= MAX_SPEED { None } else { d.m.current_instr().map(|i| (i.line, kind)) };
                let prev = if d.m.steps > 0 { d.m.prog.instr_at(d.m.prev_eip).map(|i| i.line) } else { None };
                (exec, prev)
            }
            _ => (None, None),
        };
        let hits: Option<HashMap<usize, u64>> = match (&self.dbg, self.s.show_hits) {
            (Some(d), true) if !self.stale() => {
                let mut h = HashMap::new();
                for (i, n) in d.m.hits.iter().enumerate() {
                    if *n > 0 {
                        *h.entry(d.m.prog.instrs[i].line).or_insert(0) += *n;
                    }
                }
                Some(h)
            }
            _ => None,
        };
        let code_lines = self.dbg.as_ref().filter(|_| !self.stale()).map(|d| d.m.prog.line_addr.clone());
        let params = editor::Params {
            font_size: self.s.code_size,
            breakpoints: &self.bps,
            code_lines: code_lines.as_ref(),
            bp_lines: Some(&self.instr_lines),
            exec_line: exec,
            prev_line: prev,
            diags: &self.diags,
            scroll_to: self.scroll_to.take(),
            hits: hits.as_ref(),
            operand_column: self.s.align_operands.then_some(self.s.operand_column),
            comment_column: self.s.align_comments.then_some(self.s.comment_column),
            virtual_space: self.s.virtual_space,
            italic_comments: self.s.italic_comments,
            completions: self.s.autocomplete.then_some(&self.completions[..]),
        };
        let old = self.s.source.clone();
        let mut src = std::mem::take(&mut self.s.source);
        let out = egui::Frame::new().fill(BG).show(ui, |ui| editor::show(ui, &mut src, &params)).inner;
        self.s.source = src;
        self.cursor = out.cursor;
        if out.changed && self.s.source != old {
            // Dirty only while the text differs from the saved one, so undoing every edit
            // clears the mark. Trailing spaces don't count: the virtual-space cursor pads the
            // current line with spaces that go away when the cursor leaves it.
            self.s.dirty = !same_text(&self.s.source, &self.s.saved_source);
            self.shift_breakpoints(&old, &self.s.source.clone());
            self.check_only();
        }
        if let Some(l) = out.toggle_bp {
            if !self.bps.remove(&l) {
                self.bps.insert(l);
            }
        }
        if let Some(l) = out.run_to {
            self.act_run_to(&ctx, l);
        }
        // Hover help.
        if let (Some(h), Some(resp)) = (out.hover, out.response) {
            self.hover_tooltip(resp, &h);
        }
    }

    fn hover_tooltip(&self, resp: egui::Response, h: &editor::Hover) {
        let w = h.word.trim_matches('\'').to_string();
        let lw = h.word.to_ascii_lowercase();
        // Character literal.
        if h.word.starts_with('\'') && h.word.ends_with('\'') && h.word.chars().count() == 3 {
            let c = h.word.chars().nth(1).unwrap() as i64;
            resp.on_hover_ui_at_pointer(|ui| number_tooltip(ui, c));
            return;
        }
        if lw.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            if let Some(v) = asm::parse_number(&lw) {
                resp.on_hover_ui_at_pointer(|ui| number_tooltip(ui, v));
            }
            return;
        }
        if let Some(r) = Reg::parse(&lw) {
            let doc = docs::register(&lw);
            let m = self.dbg.as_ref().map(|d| &d.m);
            let fmt = self.s.num_fmt;
            resp.on_hover_ui_at_pointer(|ui| {
                ui.set_max_width(420.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(lw.to_uppercase()).family(bold()).color(SYN_REGISTER));
                    ui.label(RichText::new(format!("{}-bit register", r.size.bits())).small().color(TEXT_DIM));
                });
                if let Some(d) = doc {
                    ui.label(RichText::new(d.summary).color(TEXT_DIM));
                }
                let Some(m) = m else {
                    ui.add_space(2.0);
                    ui.label(RichText::new("Start debugging (F10 / F11) to see its value.").small().color(TEXT_FAINT));
                    return;
                };
                let v = m.reg(r);
                let prev = {
                    let full = m.prev_regs[r.full() as usize];
                    match (r.size, r.idx) {
                        (Size::D, _) => full,
                        (Size::W, _) => full & 0xFFFF,
                        (Size::B, i) if i < 4 => full & 0xFF,
                        (Size::B, _) => (full >> 8) & 0xFF,
                    }
                };
                let changed = m.steps > 0 && prev != v;
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("= {}", fmt_num(v, fmt, r.size))).family(code()).size(16.0).color(if changed { CHANGED } else { TEXT }));
                    if changed {
                        ui.label(RichText::new(format!("was {}", fmt_num(prev, fmt, r.size))).family(code()).small().color(TEXT_DIM));
                    }
                });
                register_value_grid(ui, v, r.size);
                // Pointer into data / stack.
                if r.size == Size::D {
                    let p = &m.prog;
                    let note = if (DATA_BASE..p.static_end()).contains(&v) {
                        p.data_symbol_for(v).map(|(s, o)| if o == 0 { format!("points to {s}") } else { format!("points to {s}+{o}") })
                    } else if v >= p.static_end() && v <= STACK_TOP && (r.idx == asm::ESP || r.idx == asm::EBP || v >= m.regs[asm::ESP as usize]) {
                        Some(format!("points into the stack ({} bytes below the top)", STACK_TOP - v))
                    } else {
                        None
                    };
                    if let Some(n) = note {
                        let dw = m.peek(v, 4).map(|d| format!(" — dword there = {} (0x{d:08X})", d as i32)).unwrap_or_default();
                        ui.label(RichText::new(format!("{n}{dw}")).small().color(SYN_LABEL));
                    }
                }
                // Parts of EAX / EBX / ECX / EDX.
                if r.size == Size::D && r.idx < 4 {
                    ui.separator();
                    let part = |ui: &mut Ui, name: &str, val: u32, size: Size| {
                        ui.label(RichText::new(format!("{name:<3} {:<10} {}", fmt_num(val, NumFmt::Hex, size), size.sext(val) as i32)).family(code()).size(12.5).color(TEXT_DIM));
                    };
                    part(ui, &asm::REG16[r.idx as usize].to_uppercase(), v & 0xFFFF, Size::W);
                    part(ui, &asm::REG8[r.idx as usize + 4].to_uppercase(), (v >> 8) & 0xFF, Size::B);
                    part(ui, &asm::REG8[r.idx as usize].to_uppercase(), v & 0xFF, Size::B);
                }
            });
            return;
        }
        if lw == "eip" {
            let m = self.dbg.as_ref().map(|d| &d.m);
            resp.on_hover_ui_at_pointer(|ui| {
                ui.set_max_width(380.0);
                ui.label(RichText::new("EIP").family(bold()).color(SYN_BRANCH));
                ui.label(RichText::new(docs::register("eip").map(|d| d.summary).unwrap_or("")).color(TEXT_DIM));
                if let Some(m) = m {
                    ui.separator();
                    let line = m.current_instr().map(|i| format!("  (line {})", i.line + 1)).unwrap_or_default();
                    ui.label(RichText::new(format!("= 0x{:08X}{line}", m.eip)).family(code()).size(16.0));
                }
            });
            return;
        }
        if asm::is_mnemonic(&lw) {
            if let Some(d) = docs::lookup(&lw) {
                resp.on_hover_ui_at_pointer(|ui| instr_doc_ui(ui, d));
            }
            return;
        }
        let dir_doc = match lw.as_str() {
            "db" => Some("Define bytes: db 1, 2, 'text', 0"),
            "dw" => Some("Define 16-bit words."),
            "dd" => Some("Define 32-bit double words (also addresses of labels)."),
            "resb" | "resw" | "resd" => Some("Reserve uninitialised space (in .bss): resb N bytes, resw N words, resd N dwords."),
            "equ" => Some("Defines a constant: NAME equ expression. '$ - label' gives a length."),
            "times" => Some("Repeats the following data or instruction: times 10 db 0"),
            "section" | "segment" => Some(".text = code, .data = initialised data, .bss = zeroed data"),
            "global" => Some("Exports a label (only for compatibility with NASM; ignored here)."),
            "byte" | "word" | "dword" => Some("Operand size: byte = 8 bits, word = 16 bits, dword = 32 bits. Needed when the size isn't clear, e.g. 'mov dword [x], 5'."),
            _ => None,
        };
        if let Some(t) = dir_doc {
            resp.on_hover_ui_at_pointer(|ui| {
                ui.set_max_width(360.0);
                ui.label(RichText::new(&w).family(bold()).color(SYN_DIRECTIVE));
                ui.label(t);
            });
            return;
        }
        // Built-in constants (SCREEN, KEY_LEFT, RED, ...), unless the program defines its own.
        let user_defined = self.prog().is_some_and(|p| p.symbol(&w).is_some_and(|s| s.name == w));
        if let (Some((v, desc)), false) = (asm::builtin(&w), user_defined) {
            let colour = (w.chars().all(|c| c.is_ascii_uppercase()) && v < 256 && !w.starts_with("KEY") && !w.starts_with("SCREEN")).then(|| asm::palette(v as u8));
            resp.on_hover_ui_at_pointer(|ui| {
                ui.set_max_width(380.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&w).family(bold()).color(SYN_LABEL));
                    ui.label(RichText::new("built-in constant").small().color(TEXT_DIM));
                });
                ui.horizontal(|ui| {
                    if let Some([r, g, b]) = colour {
                        let (sw, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::hover());
                        ui.painter().rect_filled(sw, 3.0, Color32::from_rgb(r, g, b));
                    }
                    ui.label(RichText::new(format!("= {v} (0x{v:X})")).family(code()));
                });
                ui.label(desc);
            });
            return;
        }
        // Symbol.
        let prog: Option<Rc<Program>> = self.prog().cloned().or_else(|| {
            let (p, d) = asm::assemble(&self.s.source);
            d.iter().all(|d| d.severity != Severity::Error).then(|| Rc::new(p))
        });
        let Some(p) = prog else { return };
        let full = if w.starts_with('.') {
            // Local label: find the enclosing global label above the hover line.
            p.symbols.iter().filter(|s| s.name.ends_with(&w) && s.line >= h.line.saturating_sub(10_000)).min_by_key(|s| s.line.abs_diff(h.line)).map(|s| s.name.clone())
        } else {
            Some(w.clone())
        };
        let Some(sym) = full.and_then(|n| p.symbol(&n).cloned()) else { return };
        let mem = self.dbg.as_ref().map(|d| (d.m.peek(sym.value, 4), (0..16).filter_map(|i| d.m.peek(sym.value + i, 1)).map(|b| b as u8).collect::<Vec<u8>>()));
        resp.on_hover_ui_at_pointer(|ui| {
            ui.set_max_width(400.0);
            let kind = match sym.kind {
                SymKind::Code => "code label",
                SymKind::Data => "data label",
                SymKind::Bss => "bss label (zero-initialised)",
                SymKind::Const => "constant (equ)",
            };
            ui.horizontal(|ui| {
                ui.label(RichText::new(&sym.name).family(bold()).color(SYN_LABEL));
                ui.label(RichText::new(kind).small().color(TEXT_DIM));
            });
            ui.label(RichText::new(format!("{} = 0x{:08X} ({})   defined on line {}", if sym.kind == SymKind::Const { "value" } else { "address" }, sym.value, sym.value as i32, sym.line + 1)).family(code()));
            if matches!(sym.kind, SymKind::Data | SymKind::Bss) {
                if let Some((Some(dw), bytes)) = mem {
                    ui.separator();
                    ui.label(RichText::new(format!("dword [{}] = {} (0x{dw:08X})", sym.name, dw as i32)).family(code()).color(CHANGED));
                    let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02X}")).collect();
                    ui.label(RichText::new(hex.join(" ")).family(code()).color(TEXT_DIM));
                    let txt: String = bytes.iter().take_while(|b| **b != 0).map(|&b| if (32..127).contains(&b) { b as char } else { '·' }).collect();
                    if !txt.is_empty() {
                        ui.label(RichText::new(format!("\"{txt}\"")).family(code()).color(SYN_STRING));
                    }
                }
            }
        });
    }

    fn inspector(&mut self, ui: &mut Ui) {
        let fmt = self.s.num_fmt;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(6.0);
            // ---------------- Registers ----------------
            let mut fmt_new = fmt;
            widgets::card(
                ui,
                "Registers",
                |ui| {
                    for (f, l) in [(NumFmt::Bin, "bin"), (NumFmt::Signed, "±dec"), (NumFmt::Dec, "dec"), (NumFmt::Hex, "hex")] {
                        if ui.selectable_label(fmt_new == f, RichText::new(l).size(11.5)).clicked() {
                            fmt_new = f;
                        }
                    }
                },
                |ui| {
                    let m = self.dbg.as_ref().map(|d| &d.m);
                    let prog = self.prog().cloned();
                    // Values a bit larger than the names (the monospace font looks smaller at the
                    // same size). Names, values and notes use different fonts, whose line boxes
                    // place the text at different heights, so every cell is drawn by hand on the
                    // baseline of the register name: one row height (the name's), all text lined up.
                    let val_size = self.s.ui_size + 1.0;
                    let name_font = egui::FontId::new(self.s.ui_size, bold());
                    let val_font = code_font(val_size);
                    let note_font = code_font(12.0);
                    let note_font_ui = egui::FontId::proportional(12.0);
                    let galley_baseline = |g: &egui::Galley| g.rows.first().and_then(|r| r.row.glyphs.first().map(|gl| r.pos.y + gl.pos.y));
                    let name_galley = ui.fonts_mut(|f| f.layout_no_wrap("X".into(), name_font.clone(), Color32::WHITE));
                    let (row_h, base) = (name_galley.size().y, galley_baseline(&name_galley).unwrap_or(0.0));
                    // One cell: text segments (text, font, colour) side by side, on the shared baseline.
                    let cell = |ui: &mut Ui, segs: &[(&str, &egui::FontId, Color32)]| -> egui::Response {
                        let mut job = egui::text::LayoutJob::default();
                        for (t, font, color) in segs {
                            job.append(t, 0.0, egui::TextFormat::simple((*font).clone(), *color));
                        }
                        let g = ui.fonts_mut(|f| f.layout_job(job));
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(g.size().x, row_h), Sense::hover());
                        let dy = galley_baseline(&g).map_or(0.0, |b| base - b);
                        ui.painter().galley(rect.min + egui::vec2(0.0, dy), g, TEXT);
                        resp
                    };
                    egui::Grid::new("regs").num_columns(3).spacing([10.0, 3.0]).min_col_width(10.0).show(ui, |ui| {
                        let order = [asm::EAX, asm::EBX, asm::ECX, asm::EDX, asm::ESI, asm::EDI, asm::EBP, asm::ESP];
                        for idx in order {
                            let name = asm::REG32[idx as usize].to_uppercase();
                            let v = m.map(|m| m.regs[idx as usize]).unwrap_or(0);
                            let changed = m.is_some_and(|m| m.steps > 0 && m.regs[idx as usize] != m.prev_regs[idx as usize]);
                            let r = cell(ui, &[(&name, &name_font, SYN_REGISTER)]);
                            let doc = docs::register(&name.to_ascii_lowercase()).map(|d| d.summary).unwrap_or("");
                            r.on_hover_text(doc);
                            let col = if m.is_none() {
                                TEXT_FAINT
                            } else if changed {
                                CHANGED
                            } else {
                                TEXT
                            };
                            let resp = if fmt == NumFmt::Bin {
                                // Leading zeros dimmed so the significant bits stand out.
                                let s = fmt_num(v, fmt, Size::D);
                                let split = s.find('1').unwrap_or(s.len());
                                let lead_col = if m.is_none() { TEXT_FAINT } else { Color32::from_rgb(78, 84, 96) };
                                cell(ui, &[(&s[..split], &val_font, lead_col), (&s[split..], &val_font, col)])
                            } else {
                                cell(ui, &[(&fmt_num(v, fmt, Size::D), &val_font, col)])
                            };
                            // Second column: the "other" format, or what a pointer points at.
                            let mut note = String::new();
                            if let (Some(p), Some(_)) = (&prog, m) {
                                if (DATA_BASE..MEM_SIZE).contains(&v) && v < p.static_end() {
                                    if let Some((s, off)) = p.data_symbol_for(v) {
                                        note = if off == 0 { format!("→ {s}") } else { format!("→ {s}+{off}") };
                                    }
                                } else if idx == asm::ESP || idx == asm::EBP {
                                    if v <= STACK_TOP && v >= p.static_end() {
                                        note = format!("stack −{}", STACK_TOP - v);
                                    }
                                }
                            }
                            // A failed system call returns -errno in EAX: name it.
                            if let Some(mm) = m {
                                let e = -(v as i32);
                                if idx == asm::EAX && mm.last_op == Some(asm::Op::Int) && (1..=40).contains(&e) {
                                    note = format!("-{}", crate::files::errno_name(e));
                                }
                            }
                            if note.is_empty() && m.is_some() {
                                note = match fmt {
                                    NumFmt::Hex => format!("{}", v as i32),
                                    _ => format!("0x{v:X}"),
                                };
                            }
                            if fmt == NumFmt::Bin {
                                // No room next to 32 bits; the tooltip has the details.
                                note.clear();
                            }
                            cell(ui, &[(&note, &note_font, TEXT_DIM)]);
                            resp.on_hover_ui(|ui| {
                                ui.label(RichText::new(format!("{name}  {}", doc)).color(TEXT));
                                ui.separator();
                                let sub = |ui: &mut Ui, n: &str, val: u32, size: Size| {
                                    ui.label(RichText::new(format!("{n:<4} 0x{:0w$X}  {}", val, size.sext(val) as i32, w = (size.bytes() * 2) as usize)).family(code()));
                                };
                                if idx < 4 {
                                    sub(ui, &asm::REG16[idx as usize].to_uppercase(), v & 0xFFFF, Size::W);
                                    sub(ui, &asm::REG8[idx as usize + 4].to_uppercase(), (v >> 8) & 0xFF, Size::B);
                                    sub(ui, &asm::REG8[idx as usize].to_uppercase(), v & 0xFF, Size::B);
                                    ui.separator();
                                }
                                number_tooltip(ui, v as i32 as i64);
                            });
                            ui.end_row();
                        }
                        // EIP
                        cell(ui, &[("EIP", &name_font, SYN_BRANCH)]);
                        let eip = m.map(|m| m.eip).unwrap_or(0);
                        let changed = m.is_some();
                        cell(ui, &[(&format!("0x{eip:08X}"), &val_font, if changed { TEXT } else { TEXT_FAINT })])
                            .on_hover_text(docs::register("eip").map(|d| d.summary).unwrap_or(""));
                        let note = match (m, &prog) {
                            (Some(m), Some(p)) if m.eip == EXIT_ADDR => "→ exit".to_string(),
                            (Some(m), Some(p)) => match (p.function_for(m.eip), m.current_instr()) {
                                (Some(f), Some(i)) => format!("{f} · line {}", i.line + 1),
                                (None, Some(i)) => format!("line {}", i.line + 1),
                                _ => String::new(),
                            },
                            _ => String::new(),
                        };
                        cell(ui, &[(&note, &note_font_ui, TEXT_DIM)]);
                        ui.end_row();
                    });
                    if let (Some(m), Some(p)) = (m, &prog) {
                        if let Some(i) = m.current_instr() {
                            ui.add_space(4.0);
                            egui::Frame::new().fill(BG).corner_radius(4.0).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("next ▸").small().color(TEXT_DIM));
                                    ui.label(RichText::new(asm::format_instr(i, p)).family(code()).color(SYN_LABEL));
                                });
                            });
                        }
                    }
                    if m.is_none() {
                        ui.add_space(4.0);
                        ui.label(RichText::new("Start debugging (F10 / F11) to see live values.").small().color(TEXT_DIM));
                    }
                },
            );
            self.s.num_fmt = fmt_new;
            ui.add_space(8.0);

            // ---------------- Flags ----------------
            let m = self.dbg.as_ref().map(|d| &d.m);
            let eflags = m.map(|m| m.eflags).unwrap_or(0x202);
            widgets::card(
                ui,
                "Flags",
                |ui| {
                    ui.label(RichText::new(format!("EFLAGS 0x{eflags:08X}")).family(code()).size(11.5).color(TEXT_DIM));
                },
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        for f in docs::FLAGS {
                            let set = eflags & f.bit != 0;
                            let changed = m.is_some_and(|m| m.steps > 0 && (m.eflags ^ m.prev_eflags) & f.bit != 0);
                            widgets::flag_chip(ui, f.name, set, changed, m.is_some()).on_hover_ui(|ui| {
                                ui.set_max_width(300.0);
                                ui.label(RichText::new(format!("{} — {} flag", f.name, f.title)).family(bold()));
                                ui.label(f.summary);
                            });
                        }
                    });
                    if let Some(m) = m {
                        // Which conditional jumps would be taken right now.
                        ui.add_space(4.0);
                        let conds = [("je", asm::Cond::E), ("jne", asm::Cond::Ne), ("jl", asm::Cond::L), ("jg", asm::Cond::G), ("jb", asm::Cond::B), ("ja", asm::Cond::A)];
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.label(RichText::new("taken:").small().color(TEXT_DIM));
                            for (n, c) in conds {
                                let t = m.cond(c);
                                ui.label(RichText::new(n).family(code()).size(12.0).color(if t { GREEN } else { TEXT_FAINT })).on_hover_text(if t { format!("{n} would jump") } else { format!("{n} would not jump") });
                            }
                        });
                    }
                },
            );
            ui.add_space(8.0);

            // ---------------- Call stack ----------------
            let mut goto: Option<usize> = None;
            if let Some(d) = &self.dbg {
                let m = &d.m;
                let p = &m.prog;
                widgets::card(ui, "Call Stack", |_| {}, |ui| {
                    let cur_line = m.current_instr().map(|i| i.line);
                    let top = p.function_for(m.eip).unwrap_or("?");
                    if let Status::Exited(c) = m.status {
                        ui.label(RichText::new(format!("(program finished, exit code {c})")).small().color(TEXT_DIM));
                        return;
                    }
                    if ui.add(egui::Label::new(RichText::new(format!("▶ {top}   line {}", cur_line.map(|l| l + 1).unwrap_or(0))).family(code()).color(YELLOW)).sense(Sense::click())).clicked() {
                        goto = cur_line;
                    }
                    for f in m.frames.iter().rev() {
                        let caller = p.function_for(f.call_site).unwrap_or("?");
                        let line = p.instr_at(f.call_site).map(|i| i.line);
                        let r = ui.add(egui::Label::new(RichText::new(format!("  {caller}   line {}", line.map(|l| l + 1).unwrap_or(0))).family(code()).color(TEXT)).sense(Sense::click()));
                        if r.clicked() {
                            goto = line;
                        }
                    }
                    if m.frames.is_empty() {
                        ui.label(RichText::new("(no calls in progress)").small().color(TEXT_DIM));
                    }
                });
                ui.add_space(8.0);
            }
            if let Some(l) = goto {
                self.scroll_to = Some(l);
            }

            // ---------------- Stack ----------------
            if let Some(d) = &self.dbg {
                let m = &d.m;
                let p = &m.prog;
                let esp = m.regs[asm::ESP as usize];
                let ebp = m.regs[asm::EBP as usize];
                widgets::card(
                    ui,
                    "Stack",
                    |ui| {
                        ui.label(RichText::new(format!("{} bytes used", STACK_TOP.saturating_sub(esp))).size(11.5).color(TEXT_DIM));
                    },
                    |ui| {
                        let start = esp.saturating_sub(8) & !3;
                        let end = STACK_TOP.min(start + 4 * 40);
                        let ret_addrs: HashSet<u32> = m.frames.iter().map(|f| f.esp).collect();
                        // Walk the EBP chain: each frame's saved EBP slot.
                        let mut saved_ebp: HashSet<u32> = HashSet::new();
                        let mut e = ebp;
                        while e >= esp && e < STACK_TOP && e % 4 == 0 && saved_ebp.len() < 1000 && saved_ebp.insert(e) {
                            let next = m.peek(e, 4).unwrap_or(0);
                            if next <= e {
                                break;
                            }
                            e = next;
                        }
                        // Current frame's arguments: from [ebp+8] up to the next frame boundary.
                        let arg_end = saved_ebp.iter().chain(ret_addrs.iter()).copied().filter(|&x| x > ebp + 4).min().unwrap_or(ebp + 8 + 16).min(ebp + 8 + 32);
                        egui::Grid::new("stack").num_columns(3).spacing([8.0, 1.0]).striped(false).show(ui, |ui| {
                            let mut a = start;
                            while a < end {
                                let v = m.peek(a, 4).unwrap_or(0);
                                let below = a < esp;
                                let written = m.last_writes.iter().any(|(wa, wl)| a < wa + wl && *wa < a + 4);
                                let mut marks = Vec::new();
                                if a == esp {
                                    marks.push(RichText::new("ESP ▶").family(bold()).size(11.5).color(ACCENT_HI));
                                }
                                if a == ebp && ebp != 0 {
                                    marks.push(RichText::new("EBP ▶").family(bold()).size(11.5).color(PURPLE));
                                }
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 3.0;
                                    ui.label(RichText::new(format!("{a:04X}")).family(code()).size(12.0).color(if below { TEXT_FAINT } else { TEXT_DIM }));
                                    for mk in marks {
                                        ui.label(mk);
                                    }
                                });
                                let vcol = if below {
                                    TEXT_FAINT
                                } else if written {
                                    CHANGED
                                } else {
                                    TEXT
                                };
                                ui.label(RichText::new(fmt_num(v, if fmt == NumFmt::Bin { NumFmt::Hex } else { fmt }, Size::D)).family(code()).size(12.5).color(vcol));
                                let note = if below {
                                    "free".to_string()
                                } else if v == EXIT_ADDR && a == STACK_TOP - 4 {
                                    "return to system (exit)".to_string()
                                } else if ret_addrs.contains(&a) || (v >= CODE_BASE && v < p.code_end() && p.instr_at(v).is_some()) {
                                    let f = p.function_for(v.saturating_sub(4)).unwrap_or("?");
                                    format!("ret → {f} (line {})", p.instr_at(v).map(|i| i.line + 1).unwrap_or(0))
                                } else if saved_ebp.contains(&a) && ebp != 0 {
                                    if a == ebp { "saved EBP (caller's frame)".to_string() } else { "saved EBP (outer frame)".to_string() }
                                } else if (DATA_BASE..p.static_end()).contains(&v) {
                                    p.data_symbol_for(v).map(|(s, o)| if o == 0 { format!("→ {s}") } else { format!("→ {s}+{o}") }).unwrap_or_default()
                                } else if ebp != 0 && ebp >= esp && a >= ebp + 8 && a < arg_end {
                                    format!("[ebp+{}] arg", a - ebp)
                                } else if ebp != 0 && a < ebp && a >= esp && ebp - a <= 64 {
                                    format!("[ebp-{}] local", ebp - a)
                                } else {
                                    String::new()
                                };
                                ui.label(RichText::new(note).size(11.5).color(TEXT_DIM));
                                ui.end_row();
                                a += 4;
                            }
                        });
                    },
                );
            }
            ui.add_space(8.0);
        });
    }

    fn bottom(&mut self, ui: &mut Ui) {
        let e = self.error_count();
        let probs = if self.diags.is_empty() { "Problems".to_string() } else { format!("Problems ({})", self.diags.len()) };
        let _ = e;
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            widgets::tabs(
                ui,
                &mut self.s.bottom_tab,
                &[
                    (BottomTab::Console, "Console".into()),
                    (BottomTab::Problems, probs),
                    (BottomTab::Memory, "Memory".into()),
                    (BottomTab::Symbols, "Symbols".into()),
                    (BottomTab::Reference, "Reference".into()),
                ],
            );
        });
        ui.separator();
        // A resizable panel grows to fit its content, so content that is even a pixel too tall
        // makes it creep up every frame. Reserve exactly the remaining space and draw the tab
        // in a child Ui clipped to it: the content can never change the panel's size.
        let rect = ui.available_rect_before_wrap();
        ui.allocate_rect(rect, Sense::hover());
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        let ui = &mut child;
        match self.s.bottom_tab {
            BottomTab::Console => self.console(ui),
            BottomTab::Problems => self.problems(ui),
            BottomTab::Memory => self.memory(ui),
            BottomTab::Symbols => self.symbols(ui),
            BottomTab::Reference => self.reference(ui),
        }
    }

    /// Output on top, input row at the bottom, each in its own fixed rect.
    fn console(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        let row_h = 36.0;
        let gap = 6.0;
        let out_rect = egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), (rect.bottom() - row_h - gap).max(rect.top())));
        let in_rect = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - row_h), rect.max);
        let mut out_ui = ui.new_child(egui::UiBuilder::new().max_rect(out_rect).layout(Layout::top_down(Align::Min)));
        out_ui.set_clip_rect(out_rect.intersect(ui.clip_rect()));
        self.console_output(&mut out_ui);
        let mut in_ui = ui.new_child(egui::UiBuilder::new().max_rect(in_rect).layout(Layout::top_down(Align::Min)));
        self.console_input_row(&mut in_ui);
    }

    fn console_output(&mut self, ui: &mut Ui) {
        egui::Frame::new().fill(BG).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                match self.dbg.as_ref().or(self.last_run.as_ref()) {
                    Some(d) => {
                        let text = d.m.output.clone();
                        if text.is_empty() && d.m.steps == 0 {
                            ui.label(RichText::new("Program output appears here.").color(TEXT_FAINT));
                        }
                        ui.add(egui::Label::new(RichText::new(text).family(code()).size(self.s.code_size - 1.0).color(TEXT)).selectable(true));
                        match &d.m.status {
                            Status::Exited(c) => {
                                ui.add_space(4.0);
                                ui.label(RichText::new(format!("— exited with code {c} after {} steps —", d.m.steps)).color(if *c == 0 { GREEN } else { ORANGE }).size(12.0));
                            }
                            Status::Fault(m) => {
                                ui.add_space(4.0);
                                ui.label(RichText::new(format!("— error: {m} —")).color(RED).size(12.0));
                            }
                            _ => {}
                        }
                    }
                    None => {
                        ui.label(RichText::new("Program output appears here. Press F5 to run.").color(TEXT_FAINT));
                    }
                }
            });
        });
    }

    fn console_input_row(&mut self, ui: &mut Ui) {
        let waiting = self.dbg.as_ref().is_some_and(|d| d.m.status == Status::WaitInput);
        ui.horizontal(|ui| {
            let id = egui::Id::unique("console_input");
            let stroke = if waiting { Stroke::new(1.5, ORANGE) } else { Stroke::new(1.0, BORDER) };
            let mut submit = false;
            egui::Frame::new().fill(BG).stroke(stroke).corner_radius(5.0).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
                ui.label(RichText::new("›").family(code()).color(if waiting { ORANGE } else { TEXT_DIM }));
                let te = ui.add(
                    egui::TextEdit::singleline(&mut self.console_input)
                        .id(id)
                        .frame(egui::Frame::NONE)
                        .font(code_font(self.s.code_size - 1.0))
                        .desired_width(ui.available_width() - 150.0)
                        .hint_text(if waiting { "The program is waiting for input — type and press Enter" } else { "Input for the program (queued until it reads)" }),
                );
                if te.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    submit = true;
                }
            });
            if self.focus_console {
                ui.memory_mut(|m| m.request_focus(id));
                self.focus_console = false;
            }
            if ui.button("Send").clicked() {
                submit = true;
            }
            if ui.button("Clear").on_hover_text("Clear the console output").clicked() {
                if let Some(d) = &mut self.dbg {
                    d.m.output.clear();
                }
            }
            if submit {
                let line = std::mem::take(&mut self.console_input);
                if let Some(d) = &mut self.dbg {
                    d.m.output.push_str(&line);
                    d.m.output.push('\n');
                    d.m.input.extend(line.bytes());
                    d.m.input.push_back(b'\n');
                    if d.m.status == Status::WaitInput && !d.running() {
                        // Was single-stepping: retry the read now.
                        d.m.status = Status::Ready;
                        d.m.step();
                        self.after_manual_step(&ui.ctx().clone());
                    } else if d.m.status == Status::WaitInput {
                        self.stop_msg = None;
                    }
                }
                ui.memory_mut(|m| m.request_focus(id));
            }
        });
    }

    fn problems(&mut self, ui: &mut Ui) {
        if self.diags.is_empty() {
            ui.label(RichText::new("✓ No problems. The code assembles cleanly.").color(GREEN));
            return;
        }
        let mut goto = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for d in &self.diags {
                let (icon, col) = if d.severity == Severity::Error { ("●", RED) } else { ("▲", YELLOW) };
                let r = ui
                    .horizontal(|ui| {
                        ui.label(RichText::new(icon).color(col));
                        ui.label(RichText::new(format!("Line {}", d.line + 1)).family(code()).color(TEXT_DIM));
                        ui.label(RichText::new(&d.message).color(TEXT));
                    })
                    .response
                    .interact(Sense::click());
                if r.hovered() {
                    ui.painter().rect_filled(r.rect.expand(2.0), 4.0, Color32::from_white_alpha(8));
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if r.clicked() {
                    goto = Some(d.line);
                }
            }
        });
        if let Some(l) = goto {
            self.goto_line(ui.ctx(), l);
        }
    }

    fn goto_line(&mut self, ctx: &egui::Context, line: usize) {
        self.scroll_to = Some(line);
        let id = editor::editor_id();
        if let Some(mut st) = egui::TextEdit::load_state(ctx, id) {
            let ci: usize = self.s.source.split('\n').take(line).map(|l| l.chars().count() + 1).sum();
            st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(ci))));
            st.store(ctx, id);
        }
        ctx.memory_mut(|m| m.request_focus(id));
    }

    fn memory(&mut self, ui: &mut Ui) {
        let Some(d) = &self.dbg else {
            ui.label(RichText::new("Start a session (F7 assemble, or F10 step) to inspect memory.").color(TEXT_DIM));
            return;
        };
        let m = &d.m;
        let p = m.prog.clone();
        let mut jump = self.mem_jump.take();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Go to").color(TEXT_DIM));
            let r = ui.add(egui::TextEdit::singleline(&mut self.mem_goto).desired_width(140.0).font(code_font(13.0)).hint_text("0x4000 or label"));
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                let t = self.mem_goto.trim();
                jump = p.symbol(t).map(|s| s.value).or_else(|| asm::parse_number(t).map(|v| v as u32));
            }
            ui.separator();
            ui.label(RichText::new("Data:").color(TEXT_DIM));
            for s in p.symbols.iter().filter(|s| matches!(s.kind, SymKind::Data | SymKind::Bss) && !s.name.contains('.')).take(24) {
                if ui.add(egui::Button::new(RichText::new(&s.name).family(code()).size(12.0).color(SYN_LABEL)).small()).on_hover_text(format!("0x{:08X}", s.value)).clicked() {
                    jump = Some(s.value);
                }
            }
            if ui.add(egui::Button::new(RichText::new("ESP").family(code()).size(12.0).color(ACCENT_HI)).small()).clicked() {
                jump = Some(m.regs[asm::ESP as usize]);
            }
        });
        ui.add_space(2.0);

        let font = code_font(13.0);
        let row_h = ui.fonts_mut(|f| f.row_height(&font)) + 2.0;
        let first_row = DATA_BASE / 16;
        let rows = ((MEM_SIZE - DATA_BASE) / 16) as usize;
        let mut sa = egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("memview");
        if let Some(a) = jump {
            let a = a.clamp(DATA_BASE, MEM_SIZE - 1);
            sa = sa.vertical_scroll_offset(((a / 16 - first_row) as f32 * (row_h + ui.spacing().item_spacing.y)).max(0.0));
        }
        let esp = m.regs[asm::ESP as usize];
        // Colour bands per data symbol.
        let sym_starts: Vec<(u32, &str)> = p.symbols.iter().filter(|s| matches!(s.kind, SymKind::Data | SymKind::Bss) && !s.name.contains('.')).map(|s| (s.value, s.name.as_str())).collect();
        let band = |a: u32| -> Option<usize> {
            if a >= p.static_end() {
                return None;
            }
            sym_starts.iter().rposition(|(v, _)| *v <= a)
        };
        sa.show_rows(ui, row_h, rows, |ui, range| {
            for r in range {
                let base = (first_row + r as u32) * 16;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let region = if base >= esp { PURPLE } else if base < p.static_end() { TEXT_DIM } else { TEXT_FAINT };
                    let label_sym = sym_starts.iter().find(|(v, _)| *v >= base && *v < base + 16).map(|(_, n)| *n);
                    ui.add_sized([62.0, row_h], egui::Label::new(RichText::new(format!("{base:04X}")).font(font.clone()).color(region)));
                    ui.add_space(8.0);
                    for i in 0..16u32 {
                        let a = base + i;
                        let b = m.mem[a as usize];
                        let written = m.last_writes.iter().any(|(wa, wl)| a >= *wa && a < wa + wl);
                        let col = if written {
                            CHANGED
                        } else if b == 0 {
                            TEXT_FAINT
                        } else if a >= esp {
                            Color32::from_rgb(200, 175, 255)
                        } else {
                            TEXT
                        };
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(24.0, row_h), Sense::hover());
                        if let Some(bi) = band(a) {
                            let tint = if bi % 2 == 0 { Color32::from_rgba_unmultiplied(80, 140, 255, 16) } else { Color32::from_rgba_unmultiplied(120, 220, 160, 14) };
                            ui.painter().rect_filled(rect, 0.0, tint);
                        }
                        if written {
                            ui.painter().rect_filled(rect.shrink(1.0), 3.0, Color32::from_rgba_unmultiplied(255, 190, 60, 40));
                        }
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, format!("{b:02X}"), font.clone(), col);
                        if i == 7 {
                            ui.add_space(8.0);
                        }
                        resp.on_hover_ui(|ui| {
                            let what = p.data_symbol_for(a).filter(|_| a < p.static_end()).map(|(s, o)| format!("{s}+{o}")).unwrap_or_else(|| if (SCREEN_BASE..SCREEN_END).contains(&a) { format!("screen pixel x={}, y={}", (a - SCREEN_BASE) % SCREEN_W, (a - SCREEN_BASE) / SCREEN_W) } else if a >= esp { "stack".into() } else { "free".into() });
                            ui.label(RichText::new(format!("0x{a:08X}  {what}")).family(code()));
                            ui.label(RichText::new(format!("byte {b} (0x{b:02X}){}", if (32..127).contains(&b) { format!(" '{}'", b as char) } else { String::new() })).family(code()));
                            if let Some(dw) = m.peek(a, 4) {
                                ui.label(RichText::new(format!("dword {} (0x{dw:08X})", dw as i32)).family(code()).color(TEXT_DIM));
                            }
                        });
                    }
                    ui.add_space(14.0);
                    let ascii: String = (0..16).map(|i| m.mem[(base + i) as usize]).map(|b| if (32..127).contains(&b) { b as char } else { '·' }).collect();
                    ui.label(RichText::new(ascii).font(font.clone()).color(SYN_STRING));
                    if (SCREEN_BASE..SCREEN_END).contains(&base) && (base - SCREEN_BASE) % SCREEN_W == 0 {
                        ui.add_space(12.0);
                        ui.label(RichText::new(format!("◀ screen row y={}", (base - SCREEN_BASE) / SCREEN_W)).size(12.0).color(SYN_REGISTER));
                    }
                    if let Some(n) = label_sym {
                        ui.add_space(12.0);
                        ui.label(RichText::new(format!("◀ {n}")).size(12.0).color(SYN_LABEL));
                    }
                });
            }
        });
    }

    fn symbols(&mut self, ui: &mut Ui) {
        let prog = self.prog().cloned().or_else(|| {
            let (p, d) = asm::assemble(&self.s.source);
            d.iter().all(|d| d.severity != Severity::Error).then(|| Rc::new(p))
        });
        let Some(p) = prog else {
            ui.label(RichText::new("Fix the errors to see the symbol table.").color(TEXT_DIM));
            return;
        };
        let mut goto = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("syms").num_columns(4).striped(true).spacing([24.0, 4.0]).show(ui, |ui| {
                for h in ["Name", "Kind", "Value", "Line"] {
                    ui.label(RichText::new(h).family(bold()).color(TEXT_DIM));
                }
                ui.end_row();
                for s in &p.symbols {
                    if ui.add(egui::Label::new(RichText::new(&s.name).family(code()).color(SYN_LABEL)).sense(Sense::click())).clicked() {
                        goto = Some(s.line);
                    }
                    let (k, c) = match s.kind {
                        SymKind::Code => ("code", SYN_BRANCH),
                        SymKind::Data => ("data", SYN_STRING),
                        SymKind::Bss => ("bss", SYN_REGISTER),
                        SymKind::Const => ("equ", SYN_NUMBER),
                    };
                    ui.label(RichText::new(k).color(c));
                    let v = if s.kind == SymKind::Const { format!("{} (0x{:X})", s.value as i32, s.value) } else { format!("0x{:08X}", s.value) };
                    ui.label(RichText::new(v).family(code()));
                    ui.label(RichText::new(format!("{}", s.line + 1)).color(TEXT_DIM));
                    ui.end_row();
                }
            });
            ui.add_space(8.0);
            ui.label(RichText::new(format!("Memory map: code 0x{:04X}–0x{:04X} · data 0x{DATA_BASE:04X}–0x{:04X} · bss 0x{:04X}–0x{:04X} · screen 0x{SCREEN_BASE:04X}–0x{:04X} · stack grows down from 0x{STACK_TOP:X}", CODE_BASE, p.code_end(), DATA_BASE + p.data.len() as u32, p.bss_base, p.static_end(), SCREEN_END - 1)).small().color(TEXT_DIM));
        });
        if let Some(l) = goto {
            self.goto_line(ui.ctx(), l);
        }
    }

    fn reference(&mut self, ui: &mut Ui) {
        let mut tab = ui.memory(|m| m.data.get_temp::<RefTab>(egui::Id::unique("ref_tab"))).unwrap_or(RefTab::Instructions);
        ui.horizontal(|ui| {
            for (t, l) in [(RefTab::Instructions, "Instructions"), (RefTab::Syscalls, "System calls"), (RefTab::Registers, "Registers & flags")] {
                if ui.selectable_label(tab == t, l).clicked() {
                    tab = t;
                }
            }
        });
        ui.memory_mut(|m| m.data.insert_temp(egui::Id::unique("ref_tab"), tab));
        ui.add_space(4.0);
        match tab {
            RefTab::Instructions => {
                ui.columns(2, |cols| {
                    let ui = &mut cols[0];
                    ui.add(egui::TextEdit::singleline(&mut self.ref_filter).hint_text("Search instructions…").desired_width(f32::INFINITY));
                    let f = self.ref_filter.to_ascii_lowercase();
                    egui::ScrollArea::vertical().id_salt("ref_list").auto_shrink([false, false]).show(ui, |ui| {
                        let mut group = "";
                        for d in docs::INSTRUCTIONS.iter().filter(|d| f.is_empty() || d.name.contains(&f) || d.summary.to_ascii_lowercase().contains(&f)) {
                            if d.group != group {
                                group = d.group;
                                ui.add_space(4.0);
                                ui.label(RichText::new(group.to_uppercase()).size(11.0).family(bold()).color(TEXT_DIM));
                            }
                            let sel = self.s.ref_selected == d.name;
                            ui.horizontal(|ui| {
                                if ui.selectable_label(sel, RichText::new(d.name).family(code()).color(SYN_MNEMONIC)).clicked() {
                                    self.s.ref_selected = d.name.to_string();
                                }
                                ui.label(RichText::new(d.summary.split('.').next().unwrap_or("")).small().color(TEXT_DIM));
                            });
                        }
                    });
                    let ui = &mut cols[1];
                    egui::ScrollArea::vertical().id_salt("ref_detail").auto_shrink([false, false]).show(ui, |ui| {
                        if let Some(d) = docs::lookup(&self.s.ref_selected) {
                            instr_doc_ui(ui, d);
                        }
                    });
                });
            }
            RefTab::Syscalls => {
                ui.label(RichText::new("Put the call number in EAX, the arguments in EBX, ECX, EDX, then execute  int 0x80").color(TEXT));
                ui.add_space(4.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("sys").num_columns(4).striped(true).spacing([20.0, 4.0]).show(ui, |ui| {
                        for h in ["EAX", "Name", "Arguments", "What it does"] {
                            ui.label(RichText::new(h).family(bold()).color(TEXT_DIM));
                        }
                        ui.end_row();
                        for s in docs::SYSCALLS {
                            ui.label(RichText::new(format!("{}", s.eax)).family(code()).color(SYN_NUMBER));
                            ui.label(RichText::new(s.name).family(code()).color(SYN_LABEL));
                            ui.label(RichText::new(s.args).family(code()).size(12.5));
                            ui.label(s.summary);
                            ui.end_row();
                        }
                    });
                    ui.add_space(10.0);
                    ui.label(RichText::new("SCREEN & KEYBOARD").size(11.0).family(bold()).color(TEXT_DIM));
                    ui.label("The screen is 128×128 pixels at SCREEN (0xC000). Pixel (x, y) is the byte at SCREEN + y*SCREEN_W + x; its value is a palette colour:");
                    ui.label(RichText::new("mov byte [SCREEN + 10*SCREEN_W + 20], RED     ; pixel (20, 10)").family(code()).color(SYN_LABEL));
                    ui.label("Palette (xterm-256): 0–15 basic colours, 16–231 an RGB cube (16 + 36·r + 6·g + b, each 0–5), 232–255 greys. Hover a swatch for its number:");
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(1.0, 1.0);
                        for i in 0..=255u8 {
                            let [r, g, b] = asm::palette(i);
                            let (sw, resp) = ui.allocate_exact_size(egui::vec2(13.0, 13.0), Sense::hover());
                            ui.painter().rect_filled(sw, 1.0, Color32::from_rgb(r, g, b));
                            resp.on_hover_text(format!("colour {i}  (#{r:02X}{g:02X}{b:02X})"));
                        }
                    });
                    ui.add_space(4.0);
                    ui.label("Built-in constants (usable anywhere a number is):");
                    egui::Grid::new("builtins").num_columns(3).striped(true).spacing([20.0, 2.0]).show(ui, |ui| {
                        for (name, v, desc) in asm::BUILTINS {
                            ui.label(RichText::new(*name).family(code()).color(SYN_LABEL));
                            ui.label(RichText::new(format!("{v}")).family(code()).color(SYN_NUMBER));
                            ui.label(RichText::new(*desc).small());
                            ui.end_row();
                        }
                    });
                    ui.label(RichText::new("Keys reach the program while the Screen window has focus (click it). Letters arrive as upper-case ASCII ('A' = 65), digits as '0'–'9'.").small().color(TEXT_DIM));
                });
            }
            RefTab::Registers => {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("regdoc").num_columns(2).striped(true).spacing([20.0, 4.0]).show(ui, |ui| {
                        for r in docs::REGISTERS {
                            ui.label(RichText::new(r.name.to_uppercase()).family(bold()).color(SYN_REGISTER));
                            ui.label(r.summary);
                            ui.end_row();
                        }
                        for f in docs::FLAGS {
                            ui.label(RichText::new(f.name).family(bold()).color(ACCENT_HI));
                            ui.label(format!("{} — {}", f.title, f.summary));
                            ui.end_row();
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("EAX is 32 bits; AX is its low 16 bits; AH and AL are the high and low bytes of AX. The same goes for EBX/BX/BH/BL, ECX/CX/CH/CL and EDX/DX/DH/DL.").color(TEXT_DIM));
                });
            }
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.confirm.is_some() {
            let mut choice: Option<u8> = None;
            egui::Modal::new(egui::Id::unique("confirm")).show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(RichText::new("Unsaved changes").family(bold()).size(16.0));
                ui.add_space(4.0);
                ui.label(format!("{} has unsaved changes. Save them before closing it?", self.doc_name()));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("Save").color(Color32::WHITE)).fill(ACCENT)).clicked() {
                        choice = Some(0);
                    }
                    if ui.button("Don't save").clicked() {
                        choice = Some(1);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(2);
                    }
                });
            });
            match choice {
                Some(0) => {
                    self.save(ctx, false);
                    if !self.s.dirty {
                        let a = self.confirm.take().unwrap();
                        self.do_pending(ctx, a);
                    }
                }
                Some(1) => {
                    let a = self.confirm.take().unwrap();
                    self.do_pending(ctx, a);
                }
                Some(2) => self.confirm = None,
                _ => {}
            }
        }
        if self.about {
            // Same layout as CL Viewer's About page: big icon + title block, credit, description.
            let icon = self
                .about_icon
                .get_or_insert_with(|| {
                    let data = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon_256.png")).unwrap_or_default();
                    let img = egui::ColorImage::from_rgba_unmultiplied([data.width as usize, data.height as usize], &data.rgba);
                    ctx.load_texture("about_icon", img, egui::TextureOptions::LINEAR)
                })
                .clone();
            let r = egui::Modal::new(egui::Id::unique("about")).frame(egui::Frame::popup(&ctx.global_style()).inner_margin(egui::Margin::same(24))).show(ctx, |ui| {
                ui.set_width(500.0);
                // Close (X) in the top-right corner.
                let top_right = ui.max_rect().right_top();
                let x_rect = egui::Rect::from_center_size(top_right + egui::vec2(-4.0, 4.0), egui::vec2(24.0, 24.0));
                let x = ui.interact(x_rect, egui::Id::unique("about_close"), Sense::click());
                let xc = if x.hovered() { Color32::WHITE } else { Color32::from_gray(150) };
                let (c, d) = (x_rect.center(), 6.0);
                ui.painter().line_segment([c + egui::vec2(-d, -d), c + egui::vec2(d, d)], Stroke::new(1.8, xc));
                ui.painter().line_segment([c + egui::vec2(-d, d), c + egui::vec2(d, -d)], Stroke::new(1.8, xc));
                if x.clicked() {
                    self.about = false;
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&icon).fit_to_exact_size(egui::vec2(88.0, 88.0)));
                    ui.add_space(14.0);
                    ui.vertical(|ui| {
                        ui.add_space(10.0);
                        ui.label(RichText::new("ASM Test Bench").size(26.0).family(bold()).color(Color32::WHITE));
                        ui.label(RichText::new("x86 assembly code execution simulator").size(14.0).color(Color32::from_gray(190)));
                        ui.label(RichText::new(format!("Version {}", crate::update::VERSION)).size(12.5).color(Color32::from_gray(140)));
                    });
                });
                ui.add_space(22.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Brought to you by").size(15.0).color(Color32::from_gray(200)));
                    ui.label(RichText::new("Dima Polyakov").size(15.0).family(bold()).color(Color32::from_rgb(230, 190, 110)));
                });
                ui.add_space(16.0);
                ui.label(
                    RichText::new(
                        "Write 32-bit x86 assembly (NASM-style Intel syntax) and watch it execute in a simulated \
                         machine, step by step: breakpoints, stepping into, over, out and back, live registers, \
                         flags, stack and memory. Nothing runs on your real CPU.",
                    )
                    .color(Color32::from_gray(170)),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "Memory (128 KiB): code at 0x1000, data at 0x4000, the 128×128 screen at 0xC000, and the stack \
                         growing down from 0x20000. Each instruction takes a 4-byte slot (no real x86 encodings).",
                    )
                    .size(12.0)
                    .color(Color32::from_gray(130)),
                );
                ui.add_space(6.0);
                ui.label(RichText::new("Built with Rust and egui.").size(12.0).color(Color32::from_gray(120)));
            });
            if r.should_close() {
                self.about = false;
            }
        }
    }

    fn show_toast(&mut self, ctx: &egui::Context) {
        let Some((msg, col, t0)) = self.toast.clone() else { return };
        let now = Self::now(ctx);
        let t0 = if t0 == 0.0 { now } else { t0 };
        if let Some(t) = &mut self.toast {
            t.2 = t0;
        }
        let age = now - t0;
        if age > 4.5 {
            self.toast = None;
            return;
        }
        let alpha = if age > 4.0 { ((4.5 - age) / 0.5) as f32 } else { 1.0 };
        let rect = ctx.content_rect();
        egui::Area::new(egui::Id::unique("toast")).fixed_pos(egui::pos2(rect.center().x, rect.bottom() - 70.0)).pivot(egui::Align2::CENTER_BOTTOM).order(egui::Order::Foreground).show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgb(40, 42, 48).gamma_multiply(alpha))
                .stroke(Stroke::new(1.0, col.gamma_multiply(0.8 * alpha)))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(14, 9))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 4.5, col.gamma_multiply(alpha));
                        ui.label(RichText::new(msg.as_str()).color(TEXT.gamma_multiply(alpha)));
                    });
                });
        });
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.s.breakpoints = self.bps.iter().copied().collect();
        // The other tabs, in order, and where the active one sits among them.
        self.s.other_tabs = self
            .tabs
            .iter()
            .filter_map(|t| t.parked.as_ref())
            .map(|p| SavedDoc { breakpoints: p.bps.iter().copied().collect(), caret: p.cursor, ..p.doc.clone() })
            .collect();
        self.s.active_tab = self.active_index();
        self.s.caret = self.cursor;
        eframe::set_value(storage, eframe::APP_KEY, &self.s);
        self.s.other_tabs.clear();
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if std::mem::take(&mut self.focus_editor_on_start) {
            ctx.memory_mut(|m| m.request_focus(editor::editor_id()));
        }
        if ctx.input(|i| i.time) < 6.0 {
            super::titlebar::apply();
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if self.applied.0 != self.s.code_font || self.applied.1 != self.s.ui_size {
            install_fonts(&ctx, &self.s.code_font);
            apply_style(&ctx, self.s.ui_size);
            self.applied = (self.s.code_font.clone(), self.s.ui_size);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));

        if std::mem::take(&mut self.run_on_start) {
            self.act_run(&ctx);
        }
        self.shortcuts(&ctx);
        self.tick(&ctx);

        egui::Panel::top("toolbar").frame(egui::Frame::new().fill(BAR).inner_margin(egui::Margin::symmetric(10, 7))).show(ui, |ui| self.toolbar(ui));

        let status_fill = match &self.dbg {
            None => BAR,
            Some(d) if d.running() => Color32::from_rgb(0, 90, 160),
            Some(d) => match d.m.status {
                Status::Fault(_) => Color32::from_rgb(130, 35, 35),
                Status::Exited(_) => Color32::from_rgb(30, 80, 50),
                _ => Color32::from_rgb(120, 80, 10),
            },
        };
        egui::Panel::bottom("status").exact_size(26.0).frame(egui::Frame::new().fill(status_fill).inner_margin(egui::Margin::symmetric(10, 2))).show(ui, |ui| self.status_bar(ui));

        // Binary needs 35 monospace columns; widen the inspector so it fits at full size.
        let inspector_min = if self.s.num_fmt == NumFmt::Bin {
            let cw = ctx.fonts_mut(|f| f.glyph_width(&egui::FontId::new(self.s.ui_size, code()), '0'));
            35.0 * cw + 120.0
        } else {
            280.0
        };
        egui::Panel::right("inspector")
            .resizable(true)
            .default_size(370.0)
            .min_size(inspector_min)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin::symmetric(10, 4)))
            .show(ui, |ui| self.inspector(ui));

        egui::Panel::bottom("bottom")
            .resizable(true)
            .default_size(290.0)
            .min_size(120.0)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin { left: 10, right: 10, top: 4, bottom: 8 }))
            .show(ui, |ui| self.bottom(ui));

        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(ui, |ui| self.editor_area(ui));

        // Screen window: opens by itself the first time a program draws.
        if let Some(d) = self.dbg.as_ref().or(self.last_run.as_ref()) {
            if d.m.screen_used && !self.screen_auto_opened {
                self.s.screen_open = true;
                self.screen_auto_opened = true;
            }
        }
        let running = self.dbg.as_ref().is_some_and(|d| d.running());
        // A finished program's picture stays on the screen.
        let machine = self.dbg.as_mut().or(self.last_run.as_mut()).map(|d| &mut d.m);
        self.screen.show(&ctx, &mut self.s.screen_open, machine, running);

        self.update_housekeeping(&ctx);
        self.dialogs(&ctx);
        self.update_dialog(&ctx);
        self.show_toast(&ctx);
        self.keep_editor_focused(&ctx);
    }
}

impl App {
    /// The caret belongs in the editor: when nothing has the keyboard focus (a menu or dialog
    /// was closed, a button or an empty spot was clicked), the editor gets it back. Text
    /// fields that take typing (console input, memory address, reference filter) keep it, and
    /// nothing happens while a menu / dialog is open or a mouse button is held (dragging).
    fn keep_editor_focused(&mut self, ctx: &egui::Context) {
        let free = ctx.memory(|m| m.focused().is_none());
        let busy = self.confirm.is_some() || self.about || self.update_dialog || egui::Popup::is_any_open(ctx) || ctx.input(|i| i.pointer.any_down());
        if free && !busy {
            ctx.memory_mut(|m| m.request_focus(editor::editor_id()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_format() {
        assert_eq!(fmt_num(4, NumFmt::Bin, Size::D), "00000000 00000000 00000000 00000100");
        assert_eq!(fmt_num(0xFF, NumFmt::Bin, Size::B), "11111111");
        assert_eq!(fmt_num(0x1234, NumFmt::Bin, Size::W), "00010010 00110100");
        assert_eq!(fmt_num(0xFFFF_FFFE, NumFmt::Signed, Size::D), "-2");
    }

    fn test_app() -> (App, egui::Context) {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = App::new(&cc);
        // The test runner's own arguments may have been taken for files to open.
        app.toast = None;
        (app, ctx)
    }

    fn names(a: &App) -> Vec<String> {
        (0..a.tabs.len()).map(|i| a.tab_label(i).0).collect()
    }

    #[test]
    fn tabs_open_switch_and_close() {
        let (mut a, ctx) = test_app();
        assert_eq!(names(&a), ["01_hello.asm"]);
        a.open_example(&ctx, "02_sum_loop.asm");
        a.open_example(&ctx, "03_factorial.asm");
        assert_eq!(names(&a), ["01_hello.asm", "02_sum_loop.asm", "03_factorial.asm"]);
        assert_eq!(a.doc_name(), "03_factorial.asm");
        // An edit stays with its tab.
        a.s.source.push_str("; edited\n");
        a.s.dirty = true;

        // Ctrl+Tab, release: the previous tab. Again: back (it toggles between the two).
        a.tab_switch_key(&ctx, false);
        a.end_tab_switch();
        assert_eq!(a.doc_name(), "02_sum_loop.asm");
        a.tab_switch_key(&ctx, false);
        a.end_tab_switch();
        assert_eq!(a.doc_name(), "03_factorial.asm");
        assert!(a.s.source.ends_with("; edited\n") && a.s.dirty);

        // Ctrl held: Tab, Tab goes two back; more Tabs stay on the oldest (no wrap-around).
        a.tab_switch_key(&ctx, false);
        a.tab_switch_key(&ctx, false);
        assert_eq!(a.doc_name(), "01_hello.asm");
        a.tab_switch_key(&ctx, false);
        assert_eq!(a.doc_name(), "01_hello.asm");
        // Ctrl+Shift+Tab steps back up the list.
        a.tab_switch_key(&ctx, true);
        assert_eq!(a.doc_name(), "02_sum_loop.asm");
        a.end_tab_switch();

        // Opening something already open switches to it instead of opening it twice.
        a.open_example(&ctx, "01_hello.asm");
        assert_eq!(a.tabs.len(), 3);
        assert_eq!(a.doc_name(), "01_hello.asm");

        // Closing goes to the most recently used tab; the dirty one asks first.
        a.request(&ctx, Pending::CloseTab);
        assert_eq!(names(&a), ["02_sum_loop.asm", "03_factorial.asm"]);
        assert_eq!(a.doc_name(), "02_sum_loop.asm");
        a.close_tab(&ctx, 1);
        assert!(a.confirm.is_some(), "unsaved changes: asks before closing");
        assert_eq!(a.tabs.len(), 2);
    }

    /// Runs one whole app frame with these input events.
    fn frame(a: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) {
        use eframe::App as _;
        let mut f = eframe::Frame::_new_kittest();
        let input = egui::RawInput { events, screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1500.0, 950.0))), ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| a.ui(ui, &mut f));
        out.textures_delta.clear();
    }

    fn caret(ctx: &egui::Context) -> Option<usize> {
        egui::TextEdit::load_state(ctx, editor::editor_id())?.cursor.char_range().map(|r| r.primary.index.0)
    }

    #[test]
    fn each_tab_keeps_its_caret() {
        let (mut a, ctx) = test_app();
        frame(&mut a, &ctx, vec![]);
        a.open_example(&ctx, "02_sum_loop.asm");
        frame(&mut a, &ctx, vec![]);
        // Put the caret somewhere in the middle of tab 2.
        let mut st = egui::TextEdit::load_state(&ctx, editor::editor_id()).unwrap_or_default();
        st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(50))));
        st.store(&ctx, editor::editor_id());
        frame(&mut a, &ctx, vec![]);
        assert_eq!(caret(&ctx), Some(50));
        // To tab 1 and back, with Ctrl+Tab (shortcut path) and with a few frames in between.
        let ctrl_tab = egui::Event::Key { key: egui::Key::Tab, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND };
        frame(&mut a, &ctx, vec![ctrl_tab.clone()]);
        frame(&mut a, &ctx, vec![]);
        frame(&mut a, &ctx, vec![]);
        assert_eq!(a.doc_name(), "01_hello.asm");
        frame(&mut a, &ctx, vec![ctrl_tab]);
        frame(&mut a, &ctx, vec![]);
        frame(&mut a, &ctx, vec![]);
        assert_eq!(a.doc_name(), "02_sum_loop.asm");
        assert_eq!(caret(&ctx), Some(50), "the caret is where it was left");

        // Clicking the tabs instead: to the first tab and back.
        let click = |a: &mut App, pos: egui::Pos2| {
            let btn = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
            frame(a, &ctx, vec![egui::Event::PointerMoved(pos), btn(true)]);
            frame(a, &ctx, vec![btn(false)]);
            frame(a, &ctx, vec![]);
        };
        click(&mut a, egui::pos2(40.0, 78.0)); // first tab
        assert_eq!(a.doc_name(), "01_hello.asm", "clicked the first tab");
        click(&mut a, egui::pos2(160.0, 78.0)); // second tab
        assert_eq!(a.doc_name(), "02_sum_loop.asm", "clicked the second tab");
        assert_eq!(caret(&ctx), Some(50), "the caret is still where it was left");
        assert!(ctx.memory(|m| m.has_focus(editor::editor_id())), "the editor has the keyboard focus after a tab click");

        // A newly opened document starts at the top, not at the end.
        a.open_example(&ctx, "03_factorial.asm");
        frame(&mut a, &ctx, vec![]);
        assert_eq!(caret(&ctx), Some(0));

        // A tab restored from the settings (never shown yet) gets its saved caret back.
        let doc = SavedDoc { source: "aaa\nbbbb\ncc\n".into(), caret: (1, 2), ..Default::default() };
        let id = a.new_tab_id();
        a.tabs.push(Tab { id, parked: Some(Parked::from_doc(doc)) });
        let last = a.tabs.len() - 1;
        a.switch_to(&ctx, last);
        frame(&mut a, &ctx, vec![]);
        assert_eq!(caret(&ctx), Some(6), "line 1, column 2");
        assert_eq!(a.cursor, (1, 2));
    }

    #[test]
    fn editor_gets_the_focus_back_after_a_menu() {
        let (mut a, ctx) = test_app();
        for _ in 0..3 {
            frame(&mut a, &ctx, vec![]);
        }
        assert!(ctx.memory(|m| m.has_focus(editor::editor_id())), "focused on start");
        // Open a menu of the title bar with a click (found by trying along the bar), close it
        // with Esc.
        let click = |a: &mut App, pos: egui::Pos2| {
            let btn = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
            frame(a, &ctx, vec![egui::Event::PointerMoved(pos), btn(true)]);
            frame(a, &ctx, vec![btn(false)]);
            frame(a, &ctx, vec![]);
        };
        let opened = (0..40).map(|i| egui::pos2(150.0 + i as f32 * 8.0, 20.0)).any(|pos| {
            click(&mut a, pos);
            egui::Popup::is_any_open(&ctx)
        });
        assert!(opened, "a menu opened");
        assert!(!ctx.memory(|m| m.has_focus(editor::editor_id())) || egui::Popup::is_any_open(&ctx));
        let esc = egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE };
        frame(&mut a, &ctx, vec![esc]);
        frame(&mut a, &ctx, vec![]);
        frame(&mut a, &ctx, vec![]);
        assert!(!egui::Popup::is_any_open(&ctx), "closed");
        assert!(ctx.memory(|m| m.has_focus(editor::editor_id())), "the editor has the focus again");
        // Anything that drops the focus (a click on a button, an empty spot): back next frame.
        ctx.memory_mut(|m| m.surrender_focus(editor::editor_id()));
        frame(&mut a, &ctx, vec![]);
        frame(&mut a, &ctx, vec![]);
        assert!(ctx.memory(|m| m.has_focus(editor::editor_id())));
    }

    #[test]
    fn view_menu_stays_open_while_changing_settings() {
        let (mut a, ctx) = test_app();
        for _ in 0..3 {
            frame(&mut a, &ctx, vec![]);
        }
        let click = |a: &mut App, pos: egui::Pos2| {
            let btn = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
            frame(a, &ctx, vec![egui::Event::PointerMoved(pos), btn(true)]);
            frame(a, &ctx, vec![btn(false)]);
            frame(a, &ctx, vec![]);
        };
        let esc = egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE };
        // Along the title bar's menus: open each, click its first row. Only View (settings)
        // stays open; the others close after a choice.
        let mut stays_open = Vec::new();
        for i in 0..12 {
            let x = 175.0 + i as f32 * 18.0;
            click(&mut a, egui::pos2(x, 20.0));
            if !egui::Popup::is_any_open(&ctx) {
                continue;
            }
            click(&mut a, egui::pos2(x + 10.0, 52.0));
            if egui::Popup::is_any_open(&ctx) {
                stays_open.push(x);
                frame(&mut a, &ctx, vec![esc.clone()]);
                frame(&mut a, &ctx, vec![]);
            }
        }
        assert!(!stays_open.is_empty(), "a click inside View keeps it open");
        assert!(stays_open.len() < 12, "other menus still close after a choice");
        let view_x = stays_open[0];
        click(&mut a, egui::pos2(view_x, 20.0));
        click(&mut a, egui::pos2(view_x + 10.0, 52.0));
        assert!(egui::Popup::is_any_open(&ctx), "View is still open after a click inside it");
        // A click outside it (in the editor) closes it.
        click(&mut a, egui::pos2(1400.0, 900.0));
        assert!(!egui::Popup::is_any_open(&ctx), "closed by a click outside");
    }

    #[test]
    fn a_finished_program_stops_the_session_but_keeps_its_output() {
        let (mut a, ctx) = test_app(); // 01_hello.asm
        frame(&mut a, &ctx, vec![]);
        // Run (F5) until it exits.
        a.act_run(&ctx);
        for _ in 0..50 {
            frame(&mut a, &ctx, vec![]);
            if a.dbg.is_none() {
                break;
            }
        }
        assert!(a.dbg.is_none(), "the session ended by itself");
        let last = a.last_run.as_ref().expect("the finished run is kept");
        assert_eq!(last.m.output, "Hello, World!\n", "its output is still shown");
        assert!(matches!(last.m.status, Status::Exited(0)));

        // Stepping (F10) to the end does the same.
        for _ in 0..50 {
            a.act_step_over(&ctx);
            if a.dbg.is_none() {
                break;
            }
        }
        assert!(a.dbg.is_none() && a.last_run.is_some(), "stepping past the exit ends the session too");
        // The next run starts fresh.
        a.act_step_into(&ctx);
        assert!(a.dbg.is_some() && a.last_run.is_none());
    }

    #[test]
    fn program_size_is_known_without_a_session() {
        let (mut a, _ctx) = test_app(); // 01_hello.asm: 8 instructions, "Hello, World!\n" = 14 bytes
        assert!(a.dbg.is_none());
        assert_eq!(a.prog_size, (8, 14));
        a.s.source.push_str("    nop\n");
        a.check_only(); // as after every edit
        assert_eq!(a.prog_size, (9, 14), "follows the code while typing");
    }

    #[test]
    fn opened_documents_go_to_the_right_end() {
        let (mut a, ctx) = test_app();
        a.open_example(&ctx, "02_sum_loop.asm");
        a.open_example(&ctx, "03_factorial.asm");
        a.switch_to(&ctx, 0); // back to the first tab
        a.open_example(&ctx, "lesson_01_first_program.asm");
        assert_eq!(names(&a), ["01_hello.asm", "02_sum_loop.asm", "03_factorial.asm", "lesson_01_first_program.asm"]);
        assert_eq!(a.active_index(), 3, "the new tab is shown");
        // A new document opened from the first tab goes to the end too, and an untouched new
        // document is replaced by what is opened next (at the end).
        a.switch_to(&ctx, 0);
        a.request(&ctx, Pending::New);
        assert_eq!(names(&a).last().map(String::as_str), Some("untitled.asm"));
        a.open_example(&ctx, "05_array_max.asm");
        assert_eq!(names(&a), ["01_hello.asm", "02_sum_loop.asm", "03_factorial.asm", "lesson_01_first_program.asm", "05_array_max.asm"]);
    }

    #[test]
    fn last_tab_closes_to_a_new_document_and_new_replaces_a_blank_one() {
        let (mut a, ctx) = test_app();
        a.request(&ctx, Pending::CloseTab);
        assert_eq!(names(&a), ["untitled.asm"]);
        assert_eq!(a.s.source, NEW_TEMPLATE);
        // An untouched new document is simply replaced by what is opened next.
        a.open_example(&ctx, "02_sum_loop.asm");
        assert_eq!(names(&a), ["02_sum_loop.asm"]);
    }

    #[test]
    fn unsaved_mark_follows_text() {
        // Undoing back to the saved text: clean, even with virtual-space padding left on a line.
        assert!(same_text("mov eax, 1\nret\n", "mov eax, 1\nret\n"));
        assert!(same_text("mov eax, 1      \nret\n", "mov eax, 1\nret\n"));
        // Real changes: dirty (including a removed or added line).
        assert!(!same_text("mov eax, 2\nret\n", "mov eax, 1\nret\n"));
        assert!(!same_text("mov eax, 1\n\nret\n", "mov eax, 1\nret\n"));
        assert!(!same_text("  mov eax, 1\nret\n", "mov eax, 1\nret\n"));
    }
}
