//! Autocomplete: what can be typed at the caret (instructions, registers, labels, variables,
//! constants, directives), each with a short description. Pure logic; the popup is drawn by
//! the editor.

use crate::asm::{self, SymKind, Symbol};
use crate::docs;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Instruction,
    Register,
    Label,
    Variable,
    Constant,
    Directive,
    Section,
    Keyword,
}

impl Kind {
    pub fn tag(self) -> &'static str {
        match self {
            Kind::Instruction => "instruction",
            Kind::Register => "register",
            Kind::Label => "label",
            Kind::Variable => "variable",
            Kind::Constant => "constant",
            Kind::Directive => "directive",
            Kind::Section => "section",
            Kind::Keyword => "size",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Item {
    pub name: String,
    pub kind: Kind,
    /// One line shown in bold in the help: syntax, size, value or definition line.
    pub detail: String,
    /// What it does.
    pub doc: String,
}

impl Item {
    fn new(name: impl Into<String>, kind: Kind, detail: impl Into<String>, doc: impl Into<String>) -> Item {
        Item { name: name.into(), kind, detail: detail.into(), doc: doc.into() }
    }
}

/// Where the word at the caret is on its line, which decides what fits there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// Start of a statement: an instruction or a directive.
    Statement,
    /// After a data name (`msg ...`): db, dw, resb, equ, ...
    AfterName,
    /// After `section`: .text / .data / .bss.
    Section,
    /// An operand. `branch`: of a jump / call / loop, so labels come first.
    /// `mem`: inside [ ] (an address: variables first, only 32-bit registers).
    Operand { branch: bool, mem: bool },
}

/// The word being completed: chars [start, end) of the line, the typed part [start, caret).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ctx {
    pub start: usize,
    pub end: usize,
    pub prefix: String,
    pub slot: Slot,
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '$' || c == '?' || c == '@'
}

const PREFIXES: &[&str] = &["rep", "repe", "repz", "repne", "repnz"];
const DATA_DIRECTIVES: &[&str] = &["db", "dw", "dd", "resb", "resw", "resd", "equ", "times"];
const OTHER_DIRECTIVES: &[&str] = &["section", "global", "extern", "align"];

fn is_directive(w: &str) -> bool {
    DATA_DIRECTIVES.contains(&w) || OTHER_DIRECTIVES.contains(&w)
}

/// What is being typed at char column `col` of `line`; None inside a comment or string, or
/// when the caret isn't at a word (unless `allow_empty`, for an explicit Ctrl+Space).
pub fn context(line: &str, col: usize, allow_empty: bool) -> Option<Ctx> {
    let chars: Vec<char> = line.chars().collect();
    let col = col.min(chars.len());
    // Comments and strings: nothing to complete.
    let mut quote: Option<char> = None;
    for &c in &chars[..col] {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == ';' => return None,
            None if c == '"' || c == '\'' || c == '`' => quote = Some(c),
            None => {}
        }
    }
    if quote.is_some() {
        return None;
    }
    let mut start = col;
    while start > 0 && is_word_char(chars[start - 1]) {
        start -= 1;
    }
    let mut end = col;
    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }
    let prefix: String = chars[start..col].iter().collect();
    if prefix.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '$') {
        return None; // a number, or $ / $$
    }
    if prefix.is_empty() && !allow_empty {
        return None;
    }

    let before: String = chars[..start].iter().collect();
    let mut toks: Vec<String> = before.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).map(str::to_ascii_lowercase).collect();
    // Inside [ ... ] or after an operator: still an operand.
    let in_operand_expr = before.trim_end().ends_with(['[', '+', '-', '*', '(']);
    let mem = before.matches('[').count() > before.matches(']').count();
    if toks.first().is_some_and(|t| t.ends_with(':')) {
        toks.remove(0); // "label:" then a statement
    }
    while toks.first().is_some_and(|t| PREFIXES.contains(&t.as_str())) {
        toks.remove(0); // "rep movsb"
    }
    let slot = match toks.first().map(String::as_str) {
        None => Slot::Statement,
        Some("section") | Some("segment") => {
            if toks.len() == 1 {
                Slot::Section
            } else {
                return None;
            }
        }
        Some(w) if asm::is_mnemonic(w) => Slot::Operand { branch: w.starts_with('j') || w == "call" || w.starts_with("loop") || w == "jecxz", mem },
        Some(w) if is_directive(w) => Slot::Operand { branch: w == "global" || w == "extern", mem },
        Some(_) if toks.len() == 1 && !in_operand_expr => Slot::AfterName,
        Some(_) => Slot::Operand { branch: false, mem },
    };
    Some(Ctx { start, end, prefix, slot })
}

/// Instructions, registers, directives, sections, size keywords and built-in constants.
fn static_items() -> &'static [Item] {
    static ITEMS: OnceLock<Vec<Item>> = OnceLock::new();
    ITEMS.get_or_init(|| {
        let mut v = Vec::new();
        // Instructions: the documented ones, plus the other members of their families.
        let mut names: Vec<String> = docs::INSTRUCTIONS.iter().map(|d| d.name.to_string()).collect();
        for fam in ["movs", "stos", "lods", "cmps", "scas"] {
            names.extend(["w", "d"].map(|s| format!("{fam}{s}")));
        }
        let conds = ["e", "ne", "z", "nz", "g", "ge", "l", "le", "a", "ae", "b", "be", "s", "ns", "o", "no", "c", "nc"];
        for fam in ["set", "cmov"] {
            names.extend(conds.iter().map(|c| format!("{fam}{c}")));
        }
        names.sort();
        names.dedup();
        for n in names {
            if let Some(d) = docs::lookup(&n) {
                v.push(Item::new(n, Kind::Instruction, d.syntax, d.summary));
            }
        }
        for p in ["rep", "repe", "repne"] {
            v.push(Item::new(p, Kind::Instruction, format!("{p} movsb / stosb / cmpsb ..."), "Prefix: repeats the string instruction after it ECX times (repe / repne also stop on the compare result)."));
        }
        // Registers: 32-bit with their docs, 16 / 8-bit parts described from them.
        for (i, r) in asm::REG32.iter().enumerate() {
            let doc = docs::register(r).map(|d| d.summary).unwrap_or("");
            v.push(Item::new(*r, Kind::Register, "32-bit register", doc));
            let up = r.to_uppercase();
            v.push(Item::new(asm::REG16[i], Kind::Register, "16-bit register", format!("The low 16 bits of {up}.")));
            if i < 4 {
                v.push(Item::new(asm::REG8[i], Kind::Register, "8-bit register", format!("The low 8 bits of {up}.")));
                v.push(Item::new(asm::REG8[i + 4], Kind::Register, "8-bit register", format!("Bits 8-15 of {up}.")));
            }
        }
        let dirs: &[(&str, &str, &str)] = &[
            ("section", "section .text | .data | .bss", "Starts a section: .text for code, .data for initialised data, .bss for reserved space."),
            ("global", "global _start", "Marks a label as visible outside the file (accepted for compatibility; the entry point is _start)."),
            ("extern", "extern name", "Declares a label defined elsewhere (accepted for compatibility)."),
            ("align", "align n", "Pads with zeros up to the next multiple of n bytes."),
            ("db", "name db 1, 2, 'text', 0", "Defines bytes (8-bit values, characters, strings)."),
            ("dw", "name dw 1000, 2000", "Defines words (16-bit values)."),
            ("dd", "name dd 100000, label", "Defines doublewords (32-bit values or addresses)."),
            ("resb", "name resb 64", "Reserves bytes (in .bss: uninitialised space)."),
            ("resw", "name resw 16", "Reserves words (2 bytes each)."),
            ("resd", "name resd 16", "Reserves doublewords (4 bytes each)."),
            ("equ", "NAME equ 10", "Defines a constant: the name stands for the value, no memory is used."),
            ("times", "times 10 db 0", "Repeats the rest of the line n times."),
        ];
        for (n, syn, doc) in dirs {
            v.push(Item::new(*n, Kind::Directive, *syn, *doc));
        }
        v.push(Item::new(".text", Kind::Section, "section .text", "The code: instructions go here."));
        v.push(Item::new(".data", Kind::Section, "section .data", "Initialised data: db / dw / dd values."));
        v.push(Item::new(".bss", Kind::Section, "section .bss", "Reserved, zero-filled space: resb / resw / resd."));
        for (n, bytes) in [("byte", 1), ("word", 2), ("dword", 4)] {
            v.push(Item::new(n, Kind::Keyword, format!("{bytes} byte{}", if bytes > 1 { "s" } else { "" }), format!("Operand size: {n} [address] accesses {bytes} byte{} of memory.", if bytes > 1 { "s" } else { "" })));
        }
        for (n, val, doc) in asm::BUILTINS {
            v.push(Item::new(*n, Kind::Constant, format!("built-in = {val} (0x{val:X})"), *doc));
        }
        v
    })
}

/// Items for the program's own labels, variables and constants.
pub fn symbol_items(symbols: &[Symbol]) -> Vec<Item> {
    let mut v = Vec::new();
    for s in symbols {
        let line = s.line + 1;
        let (kind, detail, doc) = match s.kind {
            SymKind::Code => (Kind::Label, format!("label, line {line}"), "A place in the code: jump to it or call it.".to_string()),
            SymKind::Data => (Kind::Variable, format!("data at 0x{:X}, line {line}", s.value), "A variable in .data: use [name] for its value, name for its address.".to_string()),
            SymKind::Bss => (Kind::Variable, format!("reserved at 0x{:X}, line {line}", s.value), "Reserved space in .bss: use [name] for its value, name for its address.".to_string()),
            SymKind::Const => (Kind::Constant, format!("= {} (0x{:X}), line {line}", s.value as i32, s.value), "A constant (equ): the name stands for the value.".to_string()),
        };
        // Local labels are stored as "scope.local"; offer the ".local" form too.
        if let Some(dot) = s.name.find('.').filter(|&i| i > 0) {
            v.push(Item::new(&s.name[dot..], kind, format!("{detail} (in {})", &s.name[..dot]), doc.clone()));
        }
        v.push(Item::new(s.name.clone(), kind, detail, doc));
    }
    v
}

/// Everything that can be offered: the fixed items plus the program's symbols.
pub fn all_items(symbols: &[Symbol]) -> Vec<Item> {
    let mut v = static_items().to_vec();
    v.extend(symbol_items(symbols));
    v
}

fn fits(kind: Kind, slot: Slot) -> bool {
    match slot {
        Slot::Statement => matches!(kind, Kind::Instruction | Kind::Directive),
        Slot::AfterName => kind == Kind::Directive,
        Slot::Section => kind == Kind::Section,
        // Inside [ ]: no size keywords (they go before the bracket).
        Slot::Operand { mem: true, .. } => matches!(kind, Kind::Register | Kind::Label | Kind::Variable | Kind::Constant),
        Slot::Operand { .. } => matches!(kind, Kind::Register | Kind::Label | Kind::Variable | Kind::Constant | Kind::Keyword),
    }
}

/// Sort order of kinds in a slot (lower first).
fn rank(kind: Kind, slot: Slot) -> u8 {
    match slot {
        Slot::Operand { branch: true, .. } => match kind {
            Kind::Label => 0,
            Kind::Register => 1,
            _ => 2,
        },
        Slot::Operand { mem: true, .. } => match kind {
            Kind::Variable => 0,
            Kind::Register => 1,
            Kind::Constant => 2,
            _ => 3,
        },
        Slot::Operand { .. } => match kind {
            Kind::Register => 0,
            Kind::Variable => 1,
            Kind::Constant => 2,
            Kind::Keyword => 3,
            _ => 4,
        },
        Slot::Statement => match kind {
            Kind::Instruction => 0,
            _ => 1,
        },
        _ => 0,
    }
}

/// Items that complete `ctx`, best first. Empty when there is nothing to offer, or when the
/// word is already complete and nothing longer starts with it.
pub fn matches<'a>(items: &'a [Item], ctx: &Ctx) -> Vec<&'a Item> {
    let p = ctx.prefix.to_ascii_lowercase();
    let in_data_dir = matches!(ctx.slot, Slot::AfterName);
    let mut out: Vec<&Item> = items
        .iter()
        .filter(|it| fits(it.kind, ctx.slot))
        // An address can only use 32-bit registers.
        .filter(|it| !(matches!(ctx.slot, Slot::Operand { mem: true, .. }) && it.kind == Kind::Register && !asm::REG32.contains(&it.name.as_str())))
        .filter(|it| !in_data_dir || DATA_DIRECTIVES.contains(&it.name.as_str()))
        .filter(|it| it.name.to_ascii_lowercase().starts_with(&p))
        .collect();
    out.sort_by(|a, b| rank(a.kind, ctx.slot).cmp(&rank(b.kind, ctx.slot)).then_with(|| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase())));
    out.dedup_by(|a, b| a.name == b.name && a.kind == b.kind);
    if out.len() == 1 && out[0].name.eq_ignore_ascii_case(&ctx.prefix) {
        out.clear(); // already typed in full
    }
    out
}

/// Text to insert for `item`, following the case the user started typing in (MOV -> MOV)
/// for instructions, registers and keywords; names are inserted as defined.
pub fn insert_text(item: &Item, prefix: &str) -> String {
    let caseless = matches!(item.kind, Kind::Instruction | Kind::Register | Kind::Keyword | Kind::Directive | Kind::Section);
    if caseless && prefix.chars().any(|c| c.is_ascii_uppercase()) && !prefix.chars().any(|c| c.is_ascii_lowercase()) {
        item.name.to_ascii_uppercase()
    } else {
        item.name.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(line: &str, col: usize) -> Vec<String> {
        let items = all_items(&asm::assemble("section .data\nmsg db 'hi', 0\ncount dd 0\nLIMIT equ 10\nsection .text\n_start:\n.loop:\n    jmp .loop\n").0.symbols);
        let ctx = context(line, col, false).unwrap();
        matches(&items, &ctx).into_iter().map(|i| i.name.clone()).collect()
    }

    #[test]
    fn slots() {
        assert_eq!(context("    mo", 6, false).unwrap().slot, Slot::Statement);
        assert_eq!(context("start: mo", 9, false).unwrap().slot, Slot::Statement);
        assert_eq!(context("    mov eax, e", 14, false).unwrap().slot, Slot::Operand { branch: false, mem: false });
        assert_eq!(context("    jmp .l", 10, false).unwrap().slot, Slot::Operand { branch: true, mem: false });
        assert_eq!(context("    msg d", 9, false).unwrap().slot, Slot::AfterName);
        assert_eq!(context("section .d", 10, false).unwrap().slot, Slot::Section);
        assert_eq!(context("    mov eax, [co", 16, false).unwrap().slot, Slot::Operand { branch: false, mem: true });
        // Nothing in comments, strings and numbers.
        assert_eq!(context("    mov eax, 1 ; mo", 19, false), None);
        assert_eq!(context("    db 'mo", 10, false), None);
        assert_eq!(context("    mov eax, 12", 15, false), None);
    }

    #[test]
    fn instructions_and_operands() {
        let m = names("    m", 5);
        assert_eq!(m[0], "mov");
        assert!(m.contains(&"mul".to_string()) && m.contains(&"movzx".to_string()));
        assert!(m.iter().all(|n| n.starts_with('m')));
        // Registers first, then the program's variables and constants.
        let e = names("    mov eax, e", 14);
        assert!(e.starts_with(&["eax".to_string()]) && e.contains(&"esi".to_string()));
        // Inside [ ]: variables first, and only 32-bit registers.
        let m = names("    mov eax, [c", 15);
        assert_eq!(m, vec!["count", "CYAN"]);
        assert_eq!(names("    mov eax, [e", 15)[0], "eax");
        assert_eq!(names("    mov ecx, L", 14), vec!["LIMIT"]);
        // Jumps: labels first, local ones too.
        assert_eq!(names("    jmp .l", 10), vec![".loop"]);
        assert_eq!(names("    msg d", 9), vec!["db", "dd", "dw"]);
        assert_eq!(names("section .b", 10), vec![".bss"]);
    }

    #[test]
    fn complete_word_hides_popup_and_case_follows_typing() {
        assert!(names("    mul", 7).is_empty());
        let items = all_items(&[]);
        let mov = items.iter().find(|i| i.name == "mov").unwrap();
        assert_eq!(insert_text(mov, "MO"), "MOV");
        assert_eq!(insert_text(mov, "mo"), "mov");
    }
}
