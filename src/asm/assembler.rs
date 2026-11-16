//! Two-pass assembler.
//!
//! Pass 1 splits lines into statements, assigns addresses to labels and sizes data.
//! Pass 2 evaluates operands / data values (all symbols known by then) and builds instructions.

use std::cell::Cell;
use std::collections::HashMap;

use super::expr::{self, Linear, Resolve};
use super::lexer::{self, Tok, Token};
use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Sec {
    Text,
    Data,
    Bss,
}

#[derive(Clone, Debug)]
enum SymDef {
    Label(Sec, u32),
    Equ { expr: expr::Expr, here: (Sec, u32), scope: String },
}

#[derive(Clone, Debug)]
struct Sym {
    def: SymDef,
    line: usize,
}

#[derive(Debug)]
enum Kind {
    /// Data definition: unit size, items (token slices), placed at `offset` in the data image.
    Data { unit: Size, items: Vec<Vec<Token>> },
    Instr { mnemonic: String, mcols: (usize, usize), rep: Rep, operands: Vec<Vec<Token>> },
}

#[derive(Debug)]
struct Item {
    line: usize,
    scope: String,
    kind: Kind,
    /// Repeat count from `times`.
    count: u32,
    /// Data: byte offset in the data image (per repetition: `size`). Instr: first index.
    offset: u32,
    size: u32,
}

type Err = ((usize, usize), String);

fn e<T>(cols: (usize, usize), msg: impl Into<String>) -> Result<T, Err> {
    Err((cols, msg.into()))
}

const DATA_DIRECTIVES: &[&str] = &["db", "dw", "dd", "dq", "resb", "resw", "resd", "resq", "equ", "times"];

fn unit_of(d: &str) -> Option<Size> {
    match d {
        "db" | "resb" => Some(Size::B),
        "dw" | "resw" => Some(Size::W),
        "dd" | "resd" => Some(Size::D),
        _ => None,
    }
}

struct Ctx<'a> {
    syms: &'a HashMap<String, Sym>,
    bss_base: Option<u32>,
    here: i64,
    sect_start: i64,
    scope: &'a str,
    depth: &'a Cell<u32>,
}

fn sec_addr(sec: Sec, off: u32, bss_base: Option<u32>) -> Result<i64, String> {
    Ok(match sec {
        Sec::Text => (CODE_BASE + off * INSTR_SIZE) as i64,
        Sec::Data => (DATA_BASE + off) as i64,
        Sec::Bss => match bss_base {
            Some(b) => (b + off) as i64,
            None => return Err("address of a .bss label isn't known yet at this point".into()),
        },
    })
}

fn full_name(name: &str, scope: &str) -> String {
    if name.starts_with('.') && !name.starts_with("..") {
        format!("{scope}{name}")
    } else {
        name.to_string()
    }
}

impl Resolve for Ctx<'_> {
    fn symbol(&self, name: &str) -> Result<i64, String> {
        let full = full_name(name, self.scope);
        let Some(sym) = self.syms.get(&full) else {
            if let Some((v, _)) = builtin(&full) {
                return Ok(v as i64);
            }
            if let Some(k) = self.syms.keys().find(|k| k.eq_ignore_ascii_case(&full)) {
                return Err(format!("undefined symbol '{name}' (labels are case-sensitive; did you mean '{k}'?)"));
            }
            if Reg::parse(name).is_none() && is_mnemonic(name) {
                return Err(format!("'{name}' is an instruction, not a symbol"));
            }
            return Err(format!("undefined symbol '{name}'"));
        };
        match &sym.def {
            SymDef::Label(sec, off) => sec_addr(*sec, *off, self.bss_base),
            SymDef::Equ { expr, here, scope } => {
                if self.depth.get() > 32 {
                    return Err(format!("'{name}' is defined in terms of itself"));
                }
                self.depth.set(self.depth.get() + 1);
                let here_v = sec_addr(here.0, here.1, self.bss_base)?;
                let sub = Ctx { syms: self.syms, bss_base: self.bss_base, here: here_v, sect_start: sect_start(here.0, self.bss_base), scope, depth: self.depth };
                let r = expr::eval(expr, &sub);
                self.depth.set(self.depth.get() - 1);
                match r {
                    Ok(l) if l.regs.is_empty() => Ok(l.konst),
                    Ok(_) => Err(format!("'{name}' contains a register")),
                    Err(err) => Err(err.message),
                }
            }
        }
    }
    fn here(&self) -> i64 {
        self.here
    }
    fn sect_start(&self) -> i64 {
        self.sect_start
    }
}

fn sect_start(sec: Sec, bss_base: Option<u32>) -> i64 {
    match sec {
        Sec::Text => CODE_BASE as i64,
        Sec::Data => DATA_BASE as i64,
        Sec::Bss => bss_base.unwrap_or(0) as i64,
    }
}

fn eval_const(toks: &[Token], ctx: &Ctx) -> Result<i64, Err> {
    if toks.is_empty() {
        return e((0, 0), "expected a value");
    }
    let (ex, n) = expr::parse(toks).map_err(|x| (x.cols, x.message))?;
    if n < toks.len() {
        return e(toks[n].cols, "unexpected text after expression");
    }
    let l = expr::eval(&ex, ctx).map_err(|x| (x.cols, x.message))?;
    if let Some((_, _, c)) = l.regs.first() {
        return e(*c, "a register can't be used here");
    }
    Ok(l.konst)
}

fn split_commas(toks: &[Token]) -> Vec<Vec<Token>> {
    let mut out = vec![Vec::new()];
    let mut depth = 0i32;
    for t in toks {
        match t.tok {
            Tok::LBrack | Tok::LParen => depth += 1,
            Tok::RBrack | Tok::RParen => depth -= 1,
            Tok::Comma if depth == 0 => {
                out.push(Vec::new());
                continue;
            }
            _ => {}
        }
        out.last_mut().unwrap().push(t.clone());
    }
    if out.len() == 1 && out[0].is_empty() {
        out.clear();
    }
    out
}

fn ident(t: Option<&Token>) -> Option<String> {
    match t {
        Some(Token { tok: Tok::Ident(s), .. }) => Some(s.to_ascii_lowercase()),
        _ => None,
    }
}

pub fn is_mnemonic(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    parse_mnemonic(&l).is_some() || matches!(l.as_str(), "rep" | "repe" | "repz" | "repne" | "repnz")
}

/// Mnemonic -> (operation, size implied by the mnemonic).
pub fn parse_mnemonic(m: &str) -> Option<(Op, Option<Size>)> {
    let op = match m {
        "mov" => Op::Mov,
        "movzx" => Op::Movzx,
        "movsx" => Op::Movsx,
        "lea" => Op::Lea,
        "xchg" => Op::Xchg,
        "add" => Op::Add,
        "adc" => Op::Adc,
        "sub" => Op::Sub,
        "sbb" => Op::Sbb,
        "inc" => Op::Inc,
        "dec" => Op::Dec,
        "neg" => Op::Neg,
        "cmp" => Op::Cmp,
        "mul" => Op::Mul,
        "imul" => Op::Imul,
        "div" => Op::Div,
        "idiv" => Op::Idiv,
        "cbw" => Op::Cbw,
        "cwde" => Op::Cwde,
        "cwd" => Op::Cwd,
        "cdq" => Op::Cdq,
        "and" => Op::And,
        "or" => Op::Or,
        "xor" => Op::Xor,
        "not" => Op::Not,
        "test" => Op::Test,
        "shl" | "sal" => Op::Shl,
        "shr" => Op::Shr,
        "sar" => Op::Sar,
        "rol" => Op::Rol,
        "ror" => Op::Ror,
        "push" => Op::Push,
        "pop" => Op::Pop,
        "pushad" | "pusha" => Op::Pushad,
        "popad" | "popa" => Op::Popad,
        "pushfd" | "pushf" => Op::Pushfd,
        "popfd" | "popf" => Op::Popfd,
        "call" => Op::Call,
        "ret" => Op::Ret,
        "jmp" => Op::Jmp,
        "loop" => Op::Loop,
        "loope" | "loopz" => Op::Loope,
        "loopne" | "loopnz" => Op::Loopne,
        "rcl" => Op::Rcl,
        "rcr" => Op::Rcr,
        "bt" => Op::Bt,
        "bts" => Op::Bts,
        "btr" => Op::Btr,
        "btc" => Op::Btc,
        "bsf" => Op::Bsf,
        "bsr" => Op::Bsr,
        "popcnt" => Op::Popcnt,
        "shld" => Op::Shld,
        "shrd" => Op::Shrd,
        "bswap" => Op::Bswap,
        "xadd" => Op::Xadd,
        "cmpxchg" => Op::Cmpxchg,
        "lahf" => Op::Lahf,
        "sahf" => Op::Sahf,
        "xlat" | "xlatb" => Op::Xlat,
        "enter" => Op::Enter,
        "jecxz" => Op::Jecxz,
        "leave" => Op::Leave,
        "nop" => Op::Nop,
        "hlt" => Op::Hlt,
        "int" => Op::Int,
        "int3" => return Some((Op::Int, Some(Size::B))),
        "clc" => Op::Clc,
        "stc" => Op::Stc,
        "cmc" => Op::Cmc,
        "cld" => Op::Cld,
        "std" => Op::Std,
        _ => {
            let string_op = |p: &str, op: Op| -> Option<(Op, Option<Size>)> {
                let sz = match m.strip_prefix(p)? {
                    "b" => Size::B,
                    "w" => Size::W,
                    "d" => Size::D,
                    _ => return None,
                };
                Some((op, Some(sz)))
            };
            if let Some(r) = string_op("movs", Op::Movs).or_else(|| string_op("stos", Op::Stos)).or_else(|| string_op("lods", Op::Lods)).or_else(|| string_op("cmps", Op::Cmps)).or_else(|| string_op("scas", Op::Scas)) {
                return Some(r);
            }
            if let Some(c) = m.strip_prefix("cmov").and_then(Cond::parse) {
                return Some((Op::Cmovcc(c), None));
            }
            if let Some(c) = m.strip_prefix("set").and_then(Cond::parse) {
                return Some((Op::Setcc(c), None));
            }
            if let Some(c) = m.strip_prefix('j').and_then(Cond::parse) {
                return Some((Op::Jcc(c), None));
            }
            return None;
        }
    };
    Some((op, None))
}

/// Instructions from other assemblers / languages that students often type.
const FOREIGN: &[(&str, &str)] = &[
    ("move", "mov"),
    ("ld", "mov"),
    ("load", "mov"),
    ("li", "mov"),
    ("la", "lea"),
    ("st", "mov"),
    ("store", "mov"),
    ("jump", "jmp"),
    ("goto", "jmp"),
    ("b", "jmp"),
    ("bra", "jmp"),
    ("jal", "call"),
    ("bl", "call"),
    ("jsr", "call"),
    ("rts", "ret"),
    ("return", "ret"),
    ("beq", "je"),
    ("bne", "jne"),
    ("halt", "hlt"),
    ("print", "int 0x80 (see the Reference: System calls)"),
    ("syscall", "int 0x80"),
    ("subtract", "sub"),
    ("multiply", "imul"),
    ("divide", "idiv"),
];

/// A likely intended mnemonic for an unknown one: a known foreign alias, or a real mnemonic
/// within one or two typos.
fn suggest_mnemonic(m: &str) -> Option<String> {
    if let Some((_, s)) = FOREIGN.iter().find(|(f, _)| *f == m) {
        return Some(s.to_string());
    }
    fn dist(a: &str, b: &str) -> usize {
        let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
        let mut prev: Vec<usize> = (0..=b.len()).collect();
        for i in 1..=a.len() {
            let mut cur = vec![i; b.len() + 1];
            for j in 1..=b.len() {
                let cost = usize::from(a[i - 1] != b[j - 1]);
                cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            }
            prev = cur;
        }
        prev[b.len()]
    }
    let max = if m.len() <= 3 { 1 } else { 2 };
    crate::docs::INSTRUCTIONS
        .iter()
        .map(|d| d.name)
        .chain(["jz", "jnz", "jc", "jnc", "sal", "movsw", "movsd", "stosd", "lodsd"])
        .map(|n| (dist(m, n), n))
        .filter(|(d, _)| *d <= max)
        .min_by_key(|(d, _)| *d)
        .map(|(_, n)| n.to_string())
}

pub fn assemble(src: &str) -> (Program, Vec<Diagnostic>) {
    let mut diags: Vec<Diagnostic> = Vec::new();
    let mut err = |line: usize, cols: (usize, usize), msg: String| diags.push(Diagnostic { line, cols, message: msg, severity: Severity::Error });
    let mut warnings: Vec<Diagnostic> = Vec::new();
    let mut warn = |line: usize, cols: (usize, usize), msg: String| warnings.push(Diagnostic { line, cols, message: msg, severity: Severity::Warning });

    let mut syms: HashMap<String, Sym> = HashMap::new();
    let mut items: Vec<Item> = Vec::new();
    let mut sec = Sec::Text;
    let (mut text_lc, mut data_lc, mut bss_lc) = (0u32, 0u32, 0u32);
    let mut scope = String::new();
    let depth = Cell::new(0);
    let mut data_lines: HashMap<usize, (u32, u32)> = HashMap::new();

    // ---------------- Pass 1 ----------------
    for (ln, raw) in src.split('\n').enumerate() {
        let line = raw.trim_end_matches('\r');
        let toks = match lexer::tokenize(line) {
            Ok(t) => t,
            Err(x) => {
                err(ln, x.cols, x.message);
                continue;
            }
        };
        if toks.is_empty() {
            continue;
        }
        let mut i = 0;
        // NASM-style `[section .data]`
        let toks: Vec<Token> = if matches!(toks[0].tok, Tok::LBrack) && matches!(toks.last().map(|t| &t.tok), Some(Tok::RBrack)) && ident(toks.get(1)).is_some_and(|s| s == "section" || s == "segment") {
            toks[1..toks.len() - 1].to_vec()
        } else {
            toks
        };

        // Label: `name:` or `name db ...` / `name equ ...`.
        let mut label: Option<(String, (usize, usize))> = None;
        if let Some(Token { tok: Tok::Ident(name), cols }) = toks.first() {
            let next_is_colon = matches!(toks.get(1).map(|t| &t.tok), Some(Tok::Colon));
            let next_is_data = ident(toks.get(1)).is_some_and(|n| DATA_DIRECTIVES.contains(&n.as_str()));
            let lname = name.to_ascii_lowercase();
            if next_is_colon || (next_is_data && !is_mnemonic(name) && !matches!(lname.as_str(), "section" | "segment" | "global" | "extern" | "times")) {
                if Reg::parse(name).is_some() {
                    err(ln, *cols, format!("'{name}' is a register name and can't be used as a label"));
                    continue;
                }
                label = Some((name.clone(), *cols));
                i = if next_is_colon { 2 } else { 1 };
            }
        }

        let directive = ident(toks.get(i));

        // `name equ expr`
        if directive.as_deref() == Some("equ") {
            let Some((name, cols)) = label else {
                err(ln, toks[i].cols, "'equ' needs a name before it".into());
                continue;
            };
            let rest = &toks[i + 1..];
            match expr::parse(rest) {
                Ok((ex, n)) if n == rest.len() => {
                    let here = match sec {
                        Sec::Text => (Sec::Text, text_lc),
                        Sec::Data => (Sec::Data, data_lc),
                        Sec::Bss => (Sec::Bss, bss_lc),
                    };
                    let full = full_name(&name, &scope);
                    if let Some(prev) = syms.get(&full) {
                        err(ln, cols, format!("'{name}' is already defined on line {}", prev.line + 1));
                        continue;
                    }
                    syms.insert(full, Sym { def: SymDef::Equ { expr: ex, here, scope: scope.clone() }, line: ln });
                }
                Ok((_, n)) => err(ln, rest[n].cols, "unexpected text after expression".into()),
                Err(x) => err(ln, x.cols, x.message),
            }
            continue;
        }

        // Define the label at the current location.
        if let Some((name, cols)) = &label {
            let full = full_name(name, &scope);
            if name.starts_with('.') && scope.is_empty() {
                warn(ln, *cols, format!("local label '{name}' has no parent label"));
            }
            if let Some(prev) = syms.get(&full) {
                err(ln, *cols, format!("label '{name}' is already defined on line {}", prev.line + 1));
            } else {
                // Data written in .text goes to the data segment (see below), so its label does too.
                let is_data_dir = directive.as_deref().is_some_and(|d| unit_of(d).is_some() && d.starts_with('d') || d == "times" && toks.iter().skip(i + 1).any(|t| matches!(&t.tok, Tok::Ident(s) if unit_of(&s.to_ascii_lowercase()).is_some() && s.to_ascii_lowercase().starts_with('d'))));
                let def = match sec {
                    Sec::Text if is_data_dir => SymDef::Label(Sec::Data, data_lc),
                    Sec::Text => SymDef::Label(Sec::Text, text_lc),
                    Sec::Data => SymDef::Label(Sec::Data, data_lc),
                    Sec::Bss => SymDef::Label(Sec::Bss, bss_lc),
                };
                syms.insert(full, Sym { def, line: ln });
            }
            if !name.starts_with('.') {
                scope = name.clone();
            }
        }

        let Some(dir) = directive else {
            if i < toks.len() {
                err(ln, toks[i].cols, "expected an instruction or directive".into());
            }
            continue;
        };
        let dcols = toks[i].cols;

        match dir.as_str() {
            "section" | "segment" => {
                let name = ident(toks.get(i + 1)).unwrap_or_default();
                sec = match name.as_str() {
                    ".text" | "text" | ".code" | "code" => Sec::Text,
                    ".data" | "data" | ".rodata" | "rodata" => Sec::Data,
                    ".bss" | "bss" => Sec::Bss,
                    _ => {
                        err(ln, toks.get(i + 1).map(|t| t.cols).unwrap_or(dcols), "unknown section (use .text, .data or .bss)".into());
                        continue;
                    }
                };
                continue;
            }
            "global" | "extern" | "bits" | "default" | "cpu" | "org" | "use32" => continue,
            _ => {}
        }

        // `times N <data or instruction>`
        let mut count: u32 = 1;
        let mut j = i;
        if dir == "times" {
            let rest = &toks[i + 1..];
            let here = match sec {
                Sec::Text => sec_addr(Sec::Text, text_lc, None).unwrap(),
                Sec::Data => sec_addr(Sec::Data, data_lc, None).unwrap(),
                Sec::Bss => 0,
            };
            let ctx = Ctx { syms: &syms, bss_base: None, here, sect_start: sect_start(sec, None), scope: &scope, depth: &depth };
            match expr::parse(rest) {
                Ok((ex, n)) => match expr::eval(&ex, &ctx) {
                    Ok(l) if l.regs.is_empty() => {
                        if !(0..=65536).contains(&l.konst) {
                            err(ln, dcols, format!("times count {} out of range", l.konst));
                            continue;
                        }
                        count = l.konst as u32;
                        j = i + 1 + n;
                    }
                    Ok(_) => {
                        err(ln, dcols, "times count can't use registers".into());
                        continue;
                    }
                    Err(x) => {
                        err(ln, x.cols, x.message);
                        continue;
                    }
                },
                Err(x) => {
                    err(ln, x.cols, x.message);
                    continue;
                }
            }
        }
        let Some(dir) = ident(toks.get(j)) else {
            err(ln, toks.get(j).map(|t| t.cols).unwrap_or(dcols), "expected an instruction or directive".into());
            continue;
        };
        let dcols = toks[j].cols;
        let rest = &toks[j + 1..];

        match dir.as_str() {
            "dq" | "resq" | "dt" | "do" => {
                err(ln, dcols, format!("'{dir}' (64-bit and larger data) is not supported; use dd"));
                continue;
            }
            "align" => {
                let ctx = Ctx { syms: &syms, bss_base: None, here: 0, sect_start: 0, scope: &scope, depth: &depth };
                match eval_const(rest, &ctx) {
                    Ok(a) if a > 0 && (a as u32).is_power_of_two() => {
                        let a = a as u32;
                        match sec {
                            Sec::Bss => bss_lc = bss_lc.div_ceil(a) * a,
                            Sec::Data => {
                                let pad = (data_lc.div_ceil(a) * a) - data_lc;
                                items.push(Item { line: ln, scope: scope.clone(), kind: Kind::Data { unit: Size::B, items: Vec::new() }, count: 1, offset: data_lc, size: pad });
                                data_lc += pad;
                            }
                            Sec::Text => {}
                        }
                    }
                    Ok(_) => err(ln, dcols, "align needs a power of two".into()),
                    Err((c, m)) => err(ln, c, m),
                }
                continue;
            }
            d if d.starts_with("res") && unit_of(d).is_some() => {
                let unit = unit_of(d).unwrap();
                let ctx = Ctx { syms: &syms, bss_base: None, here: 0, sect_start: 0, scope: &scope, depth: &depth };
                let n = match eval_const(rest, &ctx) {
                    Ok(n) if (0..=0x8000).contains(&n) => n as u32,
                    Ok(n) => {
                        err(ln, dcols, format!("reserve count {n} out of range"));
                        continue;
                    }
                    Err((c, m)) => {
                        err(ln, c, m);
                        continue;
                    }
                };
                let bytes = n * unit.bytes() * count;
                if sec != Sec::Bss {
                    // Reserved space outside .bss: zero-filled data.
                    if sec == Sec::Text {
                        warn(ln, dcols, "reserved space in .text is placed in the data segment".into());
                    }
                    items.push(Item { line: ln, scope: scope.clone(), kind: Kind::Data { unit: Size::B, items: Vec::new() }, count: 1, offset: data_lc, size: bytes });
                    data_lines.insert(ln, (DATA_BASE + data_lc, bytes));
                    data_lc += bytes;
                } else {
                    if let Some((name, _)) = &label {
                        let _ = name;
                    }
                    data_lines.insert(ln, (u32::MAX - bss_lc, bytes)); // fixed up after pass 1
                    bss_lc += bytes;
                }
                continue;
            }
            d if unit_of(d).is_some() => {
                let unit = unit_of(d).unwrap();
                let parts = split_commas(rest);
                if parts.is_empty() {
                    err(ln, dcols, format!("'{d}' needs at least one value"));
                    continue;
                }
                let mut size = 0;
                for p in &parts {
                    match p.as_slice() {
                        [Token { tok: Tok::Str(s), .. }] => size += (s.len() as u32).div_ceil(unit.bytes()) * unit.bytes(),
                        _ => size += unit.bytes(),
                    }
                }
                match sec {
                    Sec::Bss => {
                        err(ln, dcols, "initialised data isn't allowed in .bss (use resb/resw/resd, or put it in .data)".into());
                        continue;
                    }
                    Sec::Text => warn(ln, dcols, "data in .text is placed in the data segment".into()),
                    Sec::Data => {}
                }
                items.push(Item { line: ln, scope: scope.clone(), kind: Kind::Data { unit, items: parts }, count, offset: data_lc, size });
                data_lines.insert(ln, (DATA_BASE + data_lc, size * count));
                data_lc += size * count;
                continue;
            }
            _ => {}
        }

        // Instruction (with optional rep prefix).
        let (rep, mnem_idx) = match dir.as_str() {
            "rep" => (Rep::Rep, j + 1),
            "repe" | "repz" => (Rep::Repe, j + 1),
            "repne" | "repnz" => (Rep::Repne, j + 1),
            _ => (Rep::None, j),
        };
        let Some(m) = ident(toks.get(mnem_idx)) else {
            err(ln, dcols, "expected an instruction after the prefix".into());
            continue;
        };
        let mcols = toks[mnem_idx].cols;
        if parse_mnemonic(&m).is_none() {
            let hint = suggest_mnemonic(&m).map(|s| format!(" — did you mean '{s}'?")).unwrap_or_default();
            if label.is_none() && toks.len() == 1 && hint.is_empty() {
                err(ln, mcols, format!("unknown instruction '{m}' (if this is meant to be a label, add a colon: '{m}:')"));
            } else {
                err(ln, mcols, format!("unknown instruction '{m}'{hint}"));
            }
            continue;
        }
        if sec != Sec::Text {
            err(ln, mcols, format!("instruction '{m}' outside the .text section"));
            continue;
        }
        let operands = split_commas(&toks[mnem_idx + 1..]);
        items.push(Item { line: ln, scope: scope.clone(), kind: Kind::Instr { mnemonic: m, mcols, rep, operands }, count, offset: text_lc, size: 1 });
        text_lc += count;
    }

    // ---------------- Layout ----------------
    let bss_base = (DATA_BASE + data_lc).div_ceil(4) * 4;
    for v in data_lines.values_mut() {
        if v.0 > u32::MAX - 0x10_0000 {
            v.0 = bss_base + (u32::MAX - v.0);
        }
    }
    let mut prog = Program { bss_base, bss_size: bss_lc, data: vec![0; data_lc as usize], data_lines, ..Default::default() };
    if text_lc as usize > MAX_INSTRS {
        err(0, (0, 0), format!("program has {text_lc} instructions; the code segment holds at most {MAX_INSTRS}"));
    }
    if prog.static_end() > SCREEN_BASE {
        err(0, (0, 0), format!("data + bss use {} bytes; at most {} fit before the screen at 0x{SCREEN_BASE:X}", prog.static_end() - DATA_BASE, SCREEN_BASE - DATA_BASE));
    }

    // Resolve every symbol (equ expressions are checked here).
    let mut names: Vec<&String> = syms.keys().collect();
    names.sort();
    for name in names {
        let sym = &syms[name];
        let ctx = Ctx { syms: &syms, bss_base: Some(bss_base), here: 0, sect_start: 0, scope: "", depth: &depth };
        let kind = match &sym.def {
            SymDef::Label(Sec::Text, _) => SymKind::Code,
            SymDef::Label(Sec::Data, _) => SymKind::Data,
            SymDef::Label(Sec::Bss, _) => SymKind::Bss,
            SymDef::Equ { .. } => SymKind::Const,
        };
        match ctx.symbol(name) {
            Ok(v) => prog.symbols.push(Symbol { name: name.clone(), value: v as u32, kind, line: sym.line }),
            Err(m) => err(sym.line, (0, 0), m),
        }
    }
    prog.symbols.sort_by_key(|s| (s.value, s.line));

    // ---------------- Pass 2 ----------------
    for item in &items {
        let ln = item.line;
        match &item.kind {
            Kind::Data { unit, items: parts } => {
                if parts.is_empty() {
                    continue; // align / reserved: zeros already
                }
                let mut bytes: Vec<u8> = Vec::with_capacity(item.size as usize);
                for p in parts {
                    if let [Token { tok: Tok::Str(s), .. }] = p.as_slice() {
                        let mut b = s.clone();
                        b.resize((s.len() as u32).div_ceil(unit.bytes()) as usize * unit.bytes() as usize, 0);
                        bytes.extend(b);
                        continue;
                    }
                    if let [Token { tok: Tok::Ident(q), .. }] = p.as_slice() {
                        if q == "?" {
                            bytes.extend(std::iter::repeat_n(0, unit.bytes() as usize));
                            continue;
                        }
                    }
                    let here = (DATA_BASE + item.offset) as i64;
                    let ctx = Ctx { syms: &syms, bss_base: Some(bss_base), here, sect_start: DATA_BASE as i64, scope: &item.scope, depth: &depth };
                    match eval_const(p, &ctx) {
                        Ok(v) => {
                            if !fits(v, *unit) {
                                let c = (p[0].cols.0, p.last().unwrap().cols.1);
                                err(ln, c, format!("value {v} doesn't fit in a {}", unit.keyword()));
                            }
                            bytes.extend_from_slice(&(v as u32).to_le_bytes()[..unit.bytes() as usize]);
                        }
                        Err((c, m)) => {
                            err(ln, c, m);
                            bytes.extend(std::iter::repeat_n(0, unit.bytes() as usize));
                        }
                    }
                }
                for r in 0..item.count {
                    let at = (item.offset + r * item.size) as usize;
                    prog.data[at..at + bytes.len()].copy_from_slice(&bytes);
                }
            }
            Kind::Instr { mnemonic, mcols, rep, operands } => {
                let addr = CODE_BASE + item.offset * INSTR_SIZE;
                let ctx = Ctx { syms: &syms, bss_base: Some(bss_base), here: addr as i64, sect_start: CODE_BASE as i64, scope: &item.scope, depth: &depth };
                match build(mnemonic, *mcols, *rep, operands, &ctx, text_lc) {
                    Ok(mut ins) => {
                        ins.line = ln;
                        prog.line_addr.insert(ln, addr);
                        for r in 0..item.count {
                            ins.addr = addr + r * INSTR_SIZE;
                            prog.instrs.push(ins.clone());
                        }
                    }
                    Err((c, m)) => {
                        err(ln, c, m);
                        // Keep addresses consistent even when a line fails.
                        for r in 0..item.count {
                            prog.instrs.push(Instr { op: Op::Nop, operands: vec![], size: Size::D, rep: Rep::None, line: ln, addr: addr + r * INSTR_SIZE });
                        }
                    }
                }
            }
        }
    }

    // Entry point.
    let entry = ["_start", "main", "start"].iter().find_map(|n| prog.symbols.iter().find(|s| s.name == *n && s.kind == SymKind::Code)).map(|s| s.value);
    prog.entry = entry.unwrap_or(CODE_BASE);
    if prog.instrs.is_empty() && diags.is_empty() {
        warn(0, (0, 0), "no instructions in the program".into());
    }

    diags.extend(warnings);
    diags.sort_by_key(|d| (d.line, d.cols.0));
    (prog, diags)
}

fn fits(v: i64, size: Size) -> bool {
    let bits = size.bits();
    v >= -(1i64 << (bits - 1)) && v <= (1i64 << bits) - 1
}

/// An operand as parsed, before validation: the original value of immediates is kept to
/// check ranges.
#[derive(Clone, Copy)]
struct Opnd {
    op: Operand,
    imm: i64,
    cols: (usize, usize),
    /// Size keyword given in front (`dword 5`, `byte [x]`).
    hint: Option<Size>,
}

fn parse_operand(toks: &[Token], ctx: &Ctx) -> Result<Opnd, Err> {
    let cols = (toks[0].cols.0, toks.last().unwrap().cols.1);
    let mut i = 0;
    let mut hint = None;
    while let Some(k) = ident(toks.get(i)) {
        match k.as_str() {
            "byte" => hint = Some(Size::B),
            "word" => hint = Some(Size::W),
            "dword" => hint = Some(Size::D),
            "qword" | "tword" | "oword" => return e(toks[i].cols, format!("'{k}' operands are not supported (32-bit only)")),
            "ptr" | "short" | "near" => {}
            "far" => return e(toks[i].cols, "far jumps are not supported"),
            _ => break,
        }
        i += 1;
    }
    let toks = &toks[i..];
    if toks.is_empty() {
        return e(cols, "missing operand after size keyword");
    }
    // Memory operand.
    if matches!(toks[0].tok, Tok::LBrack) {
        let Some(close) = toks.iter().rposition(|t| matches!(t.tok, Tok::RBrack)) else {
            return e(toks[0].cols, "missing ']'");
        };
        if close != toks.len() - 1 {
            return e(toks[close + 1].cols, "unexpected text after ']'");
        }
        let mut inner = &toks[1..close];
        // Segment overrides (ds:) are accepted and ignored.
        if inner.len() > 2 && matches!(inner[1].tok, Tok::Colon) && ident(inner.first()).is_some_and(|s| matches!(s.as_str(), "ds" | "ss" | "es" | "cs")) {
            inner = &inner[2..];
        }
        if inner.is_empty() {
            return e(toks[0].cols, "empty memory operand '[]'");
        }
        let (ex, n) = expr::parse(inner).map_err(|x| (x.cols, x.message))?;
        if n < inner.len() {
            return e(inner[n].cols, "unexpected text in memory operand");
        }
        let lin = expr::eval(&ex, ctx).map_err(|x| (x.cols, x.message))?;
        let mem = to_mem(lin, cols, hint)?;
        return Ok(Opnd { op: Operand::Mem(mem), imm: 0, cols, hint });
    }
    // Register.
    if toks.len() == 1 {
        if let Tok::Ident(name) = &toks[0].tok {
            if let Some(r) = Reg::parse(name) {
                if let Some(h) = hint {
                    if h != r.size {
                        return e(cols, format!("'{}' is a {}-bit register, not {}", r.name(), r.size.bits(), h.keyword()));
                    }
                }
                return Ok(Opnd { op: Operand::Reg(r), imm: 0, cols, hint });
            }
            if matches!(name.to_ascii_lowercase().as_str(), "cs" | "ds" | "es" | "fs" | "gs" | "ss") {
                return e(cols, "segment registers are not supported");
            }
        }
    }
    // Immediate.
    let v = eval_const(toks, ctx)?;
    if !fits(v, Size::D) {
        return e(cols, format!("value {v} doesn't fit in 32 bits"));
    }
    Ok(Opnd { op: Operand::Imm(v as u32), imm: v, cols, hint })
}

fn to_mem(lin: Linear, cols: (usize, usize), size: Option<Size>) -> Result<Mem, Err> {
    // Merge repeated registers (ebx+ebx = ebx*2).
    let mut regs: Vec<(u8, i64, (usize, usize))> = Vec::new();
    for (r, k, c) in lin.regs {
        match regs.iter_mut().find(|x| x.0 == r) {
            Some(x) => x.1 += k,
            None => regs.push((r, k, c)),
        }
    }
    regs.retain(|x| x.1 != 0);
    if regs.len() > 2 {
        return e(cols, "an address can use at most two registers (base + index*scale)");
    }
    for &(_, k, c) in &regs {
        if k < 0 {
            return e(c, "registers can't be subtracted in an address");
        }
    }
    let ok_scale = |k: i64| matches!(k, 1 | 2 | 4 | 8);
    let (mut base, mut index, mut scale) = (None, None, 1u8);
    match regs.as_slice() {
        [] => {}
        [(r, 1, _)] => base = Some(*r),
        // reg*3 / *5 / *9 = reg + reg*2 / *4 / *8
        [(r, k @ (3 | 5 | 9), _)] => {
            base = Some(*r);
            index = Some(*r);
            scale = (*k - 1) as u8;
        }
        [(r, k, c)] => {
            if !ok_scale(*k) {
                return e(*c, "scale must be 1, 2, 4 or 8");
            }
            index = Some(*r);
            scale = *k as u8;
        }
        [(a, ka, ca), (b, kb, cb)] => {
            let (bse, idx, k, c) = if *ka == 1 && (*kb != 1 || *b != ESP) {
                (*a, *b, *kb, *cb)
            } else if *kb == 1 {
                (*b, *a, *ka, *ca)
            } else {
                return e(*ca, "one of the two registers must have scale 1");
            };
            if !ok_scale(k) {
                return e(c, "scale must be 1, 2, 4 or 8");
            }
            base = Some(bse);
            index = Some(idx);
            scale = k as u8;
        }
        _ => unreachable!(),
    }
    if index == Some(ESP) {
        return e(cols, "esp can't be used as an index register");
    }
    if !fits(lin.konst, Size::D) {
        return e(cols, "address offset doesn't fit in 32 bits");
    }
    Ok(Mem { base, index, scale, disp: lin.konst as u32 as i32, size })
}

fn size_of(o: &Opnd) -> Option<Size> {
    match o.op {
        Operand::Reg(r) => Some(r.size),
        Operand::Mem(m) => m.size,
        Operand::Imm(_) => None,
    }
}

fn is_rm(o: &Opnd) -> bool {
    matches!(o.op, Operand::Reg(_) | Operand::Mem(_))
}

fn build(m: &str, mcols: (usize, usize), rep: Rep, operand_toks: &[Vec<Token>], ctx: &Ctx, n_instrs: u32) -> Result<Instr, Err> {
    let (op, implied) = parse_mnemonic(m).unwrap();
    for t in operand_toks {
        if t.is_empty() {
            return e(mcols, "empty operand (stray comma?)");
        }
    }
    let mut ops: Vec<Opnd> = operand_toks.iter().map(|t| parse_operand(t, ctx)).collect::<Result<_, _>>()?;
    let n = ops.len();
    let want = |k: usize| -> Result<(), Err> {
        if n != k {
            let s = if k == 1 { "" } else { "s" };
            return e(mcols, format!("'{m}' takes {k} operand{s}, got {n}"));
        }
        Ok(())
    };

    if rep != Rep::None && !matches!(op, Op::Movs | Op::Stos | Op::Lods | Op::Cmps | Op::Scas) {
        return e(mcols, "rep prefixes can only be used with string instructions (movsb, stosb, ...)");
    }
    if matches!(op, Op::Int) && implied == Some(Size::B) {
        // int3
        want(0)?;
        return Ok(mk(op, vec![Operand::Imm(3)], Size::B, rep));
    }

    // Size consistency for two-operand forms.
    let two_sizes = |ops: &[Opnd]| -> Result<Size, Err> {
        let (a, b) = (&ops[0], &ops[1]);
        if matches!(a.op, Operand::Mem(_)) && matches!(b.op, Operand::Mem(_)) {
            return e(b.cols, "both operands can't be memory; load one into a register first");
        }
        let sa = size_of(a);
        let sb = size_of(b);
        let size = match (sa, sb) {
            (Some(x), Some(y)) if x != y => return e(b.cols, format!("operand size mismatch: {} vs {}", x.keyword(), y.keyword())),
            (Some(x), _) => x,
            (None, Some(y)) => y,
            (None, None) => {
                if let Some(h) = a.hint.or(b.hint) {
                    h
                } else {
                    return e(a.cols, "operation size not specified; add byte, word or dword (e.g. 'mov dword [x], 5')");
                }
            }
        };
        if let Operand::Imm(_) = b.op {
            if !fits(b.imm, size) {
                return e(b.cols, format!("value {} doesn't fit in a {}", b.imm, size.keyword()));
            }
        }
        Ok(size)
    };
    let set_mem_size = |ops: &mut [Opnd], s: Size| {
        for o in ops.iter_mut() {
            if let Operand::Mem(m) = &mut o.op {
                if m.size.is_none() {
                    m.size = Some(s);
                }
            }
        }
    };
    let code_target = |o: &Opnd| -> Result<(), Err> {
        if let Operand::Imm(v) = o.op {
            let end = CODE_BASE + n_instrs * INSTR_SIZE;
            if v < CODE_BASE || v >= end.max(CODE_BASE + INSTR_SIZE) || (v - CODE_BASE) % INSTR_SIZE != 0 {
                if (DATA_BASE..STACK_TOP).contains(&v) {
                    return e(o.cols, "jump target is a data address, not a code label");
                }
                return e(o.cols, "jump target is not a code label");
            }
        }
        Ok(())
    };

    let ops_out: Vec<Operand>;
    let size: Size;
    match op {
        Op::Mov | Op::Add | Op::Adc | Op::Sub | Op::Sbb | Op::Cmp | Op::And | Op::Or | Op::Xor | Op::Test => {
            want(2)?;
            if !is_rm(&ops[0]) {
                return e(ops[0].cols, format!("the first operand of '{m}' must be a register or memory"));
            }
            size = two_sizes(&ops)?;
            set_mem_size(&mut ops, size);
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Xchg => {
            want(2)?;
            if !is_rm(&ops[0]) || !is_rm(&ops[1]) {
                return e(mcols, "xchg needs two registers, or a register and memory");
            }
            size = two_sizes(&ops)?;
            set_mem_size(&mut ops, size);
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Movzx | Op::Movsx => {
            want(2)?;
            let Operand::Reg(d) = ops[0].op else { return e(ops[0].cols, format!("the destination of '{m}' must be a register")) };
            if d.size == Size::B {
                return e(ops[0].cols, format!("the destination of '{m}' must be a 16- or 32-bit register"));
            }
            if !is_rm(&ops[1]) {
                return e(ops[1].cols, format!("the source of '{m}' must be a register or memory"));
            }
            let Some(s) = size_of(&ops[1]) else { return e(ops[1].cols, "source size not specified; use 'byte [..]' or 'word [..]'") };
            if s.bytes() >= d.size.bytes() {
                return e(ops[1].cols, format!("the source of '{m}' must be smaller than the destination (use mov)"));
            }
            size = d.size;
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Lea => {
            want(2)?;
            let Operand::Reg(d) = ops[0].op else { return e(ops[0].cols, "the destination of lea must be a register") };
            if d.size == Size::B {
                return e(ops[0].cols, "lea needs a 16- or 32-bit register");
            }
            if !matches!(ops[1].op, Operand::Mem(_)) {
                return e(ops[1].cols, "lea needs a memory operand, e.g. 'lea eax, [ebx+4]'");
            }
            size = d.size;
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Inc | Op::Dec | Op::Neg | Op::Not | Op::Mul | Op::Div | Op::Idiv => {
            want(1)?;
            if !is_rm(&ops[0]) {
                return e(ops[0].cols, format!("'{m}' needs a register or memory operand"));
            }
            let Some(s) = size_of(&ops[0]).or(ops[0].hint) else { return e(ops[0].cols, "operation size not specified; add byte, word or dword") };
            size = s;
            set_mem_size(&mut ops, size);
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Imul => {
            match n {
                1 => {
                    if !is_rm(&ops[0]) {
                        return e(ops[0].cols, "imul needs a register or memory operand");
                    }
                    let Some(s) = size_of(&ops[0]).or(ops[0].hint) else { return e(ops[0].cols, "operation size not specified; add byte, word or dword") };
                    size = s;
                    set_mem_size(&mut ops, size);
                }
                2 | 3 => {
                    let Operand::Reg(d) = ops[0].op else { return e(ops[0].cols, "the destination of imul must be a register") };
                    if d.size == Size::B {
                        return e(ops[0].cols, "two/three-operand imul needs a 16- or 32-bit register");
                    }
                    size = d.size;
                    if n == 2 {
                        if let Operand::Imm(_) = ops[1].op {
                            // imul r, imm == imul r, r, imm
                            let r = ops[0];
                            ops.insert(1, r);
                        }
                    }
                    if !is_rm(&ops[1]) {
                        return e(ops[1].cols, "the second operand of imul must be a register or memory");
                    }
                    if let Some(s) = size_of(&ops[1]) {
                        if s != size {
                            return e(ops[1].cols, "operand size mismatch");
                        }
                    }
                    if ops.len() == 3 && !matches!(ops[2].op, Operand::Imm(_)) {
                        return e(ops[2].cols, "the third operand of imul must be a constant");
                    }
                    set_mem_size(&mut ops, size);
                }
                _ => return e(mcols, "imul takes 1, 2 or 3 operands"),
            }
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Shl | Op::Shr | Op::Sar | Op::Rol | Op::Ror | Op::Rcl | Op::Rcr => {
            if n == 1 {
                ops.push(Opnd { op: Operand::Imm(1), imm: 1, cols: mcols, hint: None });
            }
            if ops.len() != 2 {
                return e(mcols, format!("'{m}' takes 2 operands"));
            }
            if !is_rm(&ops[0]) {
                return e(ops[0].cols, format!("the first operand of '{m}' must be a register or memory"));
            }
            match ops[1].op {
                Operand::Imm(_) => {
                    if !(0..=255).contains(&ops[1].imm) {
                        return e(ops[1].cols, "shift count must be 0..255");
                    }
                }
                Operand::Reg(Reg { idx: ECX, size: Size::B }) => {}
                _ => return e(ops[1].cols, "the shift count must be a constant or cl"),
            }
            let Some(s) = size_of(&ops[0]).or(ops[0].hint) else { return e(ops[0].cols, "operation size not specified; add byte, word or dword") };
            size = s;
            set_mem_size(&mut ops[..1], size);
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Push | Op::Pop => {
            want(1)?;
            size = match ops[0].op {
                Operand::Reg(r) if r.size == Size::B => return e(ops[0].cols, format!("can't {m} an 8-bit register; use the 32-bit one ({})", REG32[r.full() as usize])),
                Operand::Reg(r) => r.size,
                Operand::Mem(mm) => match mm.size.unwrap_or(Size::D) {
                    Size::B => return e(ops[0].cols, format!("can't {m} a byte; use word or dword")),
                    s => s,
                },
                Operand::Imm(_) if op == Op::Pop => return e(ops[0].cols, "pop needs a register or memory destination"),
                Operand::Imm(_) => match ops[0].hint {
                    Some(Size::W) => Size::W,
                    _ => Size::D,
                },
            };
            set_mem_size(&mut ops, size);
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Call | Op::Jmp => {
            want(1)?;
            match ops[0].op {
                Operand::Imm(_) => code_target(&ops[0])?,
                Operand::Reg(r) if r.size == Size::D => {}
                Operand::Mem(_) => set_mem_size(&mut ops, Size::D),
                _ => return e(ops[0].cols, format!("'{m}' needs a label, a 32-bit register or memory")),
            }
            size = Size::D;
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Jcc(_) | Op::Loop | Op::Loope | Op::Loopne | Op::Jecxz => {
            want(1)?;
            if !matches!(ops[0].op, Operand::Imm(_)) {
                return e(ops[0].cols, format!("'{m}' needs a label"));
            }
            code_target(&ops[0])?;
            size = Size::D;
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Ret => {
            if n > 1 {
                return e(mcols, "ret takes at most one operand");
            }
            if n == 1 && !matches!(ops[0].op, Operand::Imm(_)) {
                return e(ops[0].cols, "ret's operand must be a constant (bytes to pop)");
            }
            size = Size::D;
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Int => {
            want(1)?;
            if !matches!(ops[0].op, Operand::Imm(_)) || !(0..=255).contains(&ops[0].imm) {
                return e(ops[0].cols, "int needs a constant 0..255, e.g. 'int 0x80'");
            }
            size = Size::B;
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Setcc(_) => {
            want(1)?;
            if !is_rm(&ops[0]) || size_of(&ops[0]).is_some_and(|s| s != Size::B) {
                return e(ops[0].cols, format!("'{m}' needs an 8-bit register or byte memory"));
            }
            size = Size::B;
            set_mem_size(&mut ops, size);
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Cmovcc(_) => {
            want(2)?;
            let Operand::Reg(d) = ops[0].op else { return e(ops[0].cols, format!("the destination of '{m}' must be a register")) };
            if d.size == Size::B {
                return e(ops[0].cols, "cmov needs a 16- or 32-bit register");
            }
            if !is_rm(&ops[1]) {
                return e(ops[1].cols, "the source of cmov must be a register or memory");
            }
            size = two_sizes(&ops)?;
            set_mem_size(&mut ops, size);
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Movs | Op::Stos | Op::Lods | Op::Cmps | Op::Scas => {
            want(0)?;
            size = implied.unwrap();
            ops_out = vec![];
        }
        Op::Bt | Op::Bts | Op::Btr | Op::Btc => {
            want(2)?;
            if !is_rm(&ops[0]) {
                return e(ops[0].cols, format!("the first operand of '{m}' must be a register or memory"));
            }
            size = match &ops[1].op {
                Operand::Imm(_) => {
                    if !(0..=255).contains(&ops[1].imm) {
                        return e(ops[1].cols, "the bit number must be 0..255");
                    }
                    let Some(s) = size_of(&ops[0]).or(ops[0].hint) else { return e(ops[0].cols, "operation size not specified; add word or dword") };
                    s
                }
                Operand::Reg(_) => two_sizes(&ops)?,
                Operand::Mem(_) => return e(ops[1].cols, "the bit number must be a register or a constant"),
            };
            if size == Size::B {
                return e(ops[0].cols, format!("'{m}' works on 16- or 32-bit values"));
            }
            set_mem_size(&mut ops[..1], size);
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Bsf | Op::Bsr | Op::Popcnt => {
            want(2)?;
            let Operand::Reg(d) = ops[0].op else { return e(ops[0].cols, format!("the destination of '{m}' must be a register")) };
            if d.size == Size::B {
                return e(ops[0].cols, format!("'{m}' needs a 16- or 32-bit register"));
            }
            if !is_rm(&ops[1]) {
                return e(ops[1].cols, format!("the source of '{m}' must be a register or memory"));
            }
            size = two_sizes(&ops)?;
            set_mem_size(&mut ops, size);
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Shld | Op::Shrd => {
            want(3)?;
            if !is_rm(&ops[0]) {
                return e(ops[0].cols, format!("the first operand of '{m}' must be a register or memory"));
            }
            let Operand::Reg(src) = ops[1].op else { return e(ops[1].cols, format!("the second operand of '{m}' must be a register")) };
            if src.size == Size::B {
                return e(ops[1].cols, format!("'{m}' works on 16- or 32-bit values"));
            }
            match ops[2].op {
                Operand::Imm(_) if (0..=255).contains(&ops[2].imm) => {}
                Operand::Reg(Reg { idx: ECX, size: Size::B }) => {}
                _ => return e(ops[2].cols, "the shift count must be a constant or cl"),
            }
            size = two_sizes(&ops[..2])?;
            set_mem_size(&mut ops[..1], size);
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Bswap => {
            want(1)?;
            match ops[0].op {
                Operand::Reg(r) if r.size == Size::D => {}
                _ => return e(ops[0].cols, "bswap needs a 32-bit register"),
            }
            size = Size::D;
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Xadd | Op::Cmpxchg => {
            want(2)?;
            if !is_rm(&ops[0]) {
                return e(ops[0].cols, format!("the first operand of '{m}' must be a register or memory"));
            }
            if !matches!(ops[1].op, Operand::Reg(_)) {
                return e(ops[1].cols, format!("the second operand of '{m}' must be a register"));
            }
            size = two_sizes(&ops)?;
            set_mem_size(&mut ops, size);
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        Op::Enter => {
            want(2)?;
            if !matches!(ops[0].op, Operand::Imm(_)) || !(0..=0xFFFF).contains(&ops[0].imm) {
                return e(ops[0].cols, "enter's first operand is the size of the locals: a constant 0..65535");
            }
            if !matches!(ops[1].op, Operand::Imm(_)) || ops[1].imm != 0 {
                return e(ops[1].cols, "only nesting level 0 is supported: 'enter size, 0'");
            }
            size = Size::D;
            ops_out = ops.iter().map(|o| o.op).collect();
        }
        _ => {
            want(0)?;
            size = Size::D;
            ops_out = vec![];
        }
    }
    let rep = match (op, rep) {
        (Op::Movs | Op::Stos | Op::Lods, Rep::Repe | Rep::Repne) => return e(mcols, "use 'rep' (not repe/repne) with movs/stos/lods"),
        (Op::Cmps | Op::Scas, Rep::Rep) => Rep::Repe,
        (_, r) => r,
    };
    Ok(mk(op, ops_out, size, rep))
}

fn mk(op: Op, operands: Vec<Operand>, size: Size, rep: Rep) -> Instr {
    Instr { op, operands, size, rep, line: 0, addr: 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(src: &str) -> Program {
        let (p, d) = assemble(src);
        let errs: Vec<_> = d.iter().filter(|d| d.severity == Severity::Error).collect();
        assert!(errs.is_empty(), "{errs:#?}");
        p
    }

    fn fails(src: &str) -> String {
        let (_, d) = assemble(src);
        d.into_iter().find(|d| d.severity == Severity::Error).map(|d| d.message).expect("expected an error")
    }

    #[test]
    fn hello() {
        let p = ok("section .data\nmsg db 'Hi', 10\nlen equ $ - msg\nsection .text\nglobal _start\n_start:\n mov eax, 4\n mov ecx, msg\n mov edx, len\n int 0x80\n");
        assert_eq!(p.data, b"Hi\n");
        assert_eq!(p.symbol("len").unwrap().value, 3);
        assert_eq!(p.instrs.len(), 4);
        assert_eq!(p.instrs[1].operands[1], Operand::Imm(DATA_BASE));
        assert_eq!(p.entry, CODE_BASE);
    }

    #[test]
    fn memory_operands() {
        let p = ok("section .bss\nbuf resd 4\nsection .text\nmov eax, [buf + ecx*4]\nmov byte [ebx+esi+1], 'A'\nlea eax, [ebx*2+ebx]\n");
        match p.instrs[0].operands[1] {
            Operand::Mem(m) => {
                assert_eq!(m.index, Some(ECX));
                assert_eq!(m.scale, 4);
                assert_eq!(m.disp as u32, p.bss_base);
            }
            _ => panic!(),
        }
        assert_eq!(p.instrs[1].size, Size::B);
        match p.instrs[2].operands[1] {
            Operand::Mem(m) => assert_eq!((m.index, m.scale), (Some(EBX), 2)),
            _ => panic!(),
        }
    }

    #[test]
    fn local_labels_and_forward_refs() {
        let p = ok("f:\n.l: dec ecx\n jnz .l\n jmp g\ng:\n.l: ret\n");
        assert!(p.symbol("f.l").is_some());
        assert!(p.symbol("g.l").is_some());
        assert_eq!(p.instrs[1].operands[0], Operand::Imm(CODE_BASE));
    }

    #[test]
    fn errors() {
        assert!(fails("mov [eax], 5").contains("size not specified"));
        assert!(fails("mov al, ebx").contains("mismatch"));
        assert!(fails("mov [eax], [ebx]").contains("both operands"));
        assert!(fails("jmp nowhere").contains("undefined symbol"));
        assert!(fails("mov al, 300").contains("doesn't fit"));
        assert!(fails("frobnicate eax").contains("unknown instruction"));
        assert!(fails("Loop:\njmp loop").contains("did you mean 'Loop'"));
        assert!(fails("section .data\nx dd 1\nsection .text\njmp x").contains("data address"));
    }

    #[test]
    fn data_forms() {
        let p = ok("section .data\na dw 1, 2\nb dd a\nc times 3 db 7\nd db 'ab', 0\n");
        assert_eq!(p.data[..4], [1, 0, 2, 0]);
        assert_eq!(u32::from_le_bytes(p.data[4..8].try_into().unwrap()), DATA_BASE);
        assert_eq!(p.data[8..11], [7, 7, 7]);
        assert_eq!(&p.data[11..14], b"ab\0");
    }
}

#[cfg(test)]
mod reg16_tests {
    use super::*;
    use crate::cpu::Machine;
    use std::rc::Rc;

    #[test]
    fn sixteen_and_eight_bit_registers() {
        let (p, d) = assemble("mov eax, 0x12345678\nmov ax, 0\nmov bl, 0x7F\nmov bh, 1\nadd ax, 0xFFFF\nhlt");
        assert!(d.is_empty(), "{d:#?}");
        let mut m = Machine::new(Rc::new(p));
        for _ in 0..6 {
            m.step();
        }
        assert_eq!(m.regs[EAX as usize], 0x1234_FFFF); // upper 16 bits kept
        assert_eq!(m.regs[EBX as usize] & 0xFFFF, 0x017F);
    }

    #[test]
    fn move_typo() {
        let msg = |s: &str| assemble(s).1.remove(0).message;
        assert!(msg("move ax, 0").contains("did you mean 'mov'"));
        assert!(msg("pussh eax").contains("did you mean 'push'"));
        assert!(msg("jump done\ndone: ret").contains("did you mean 'jmp'"));
        assert!(!msg("frobnicate eax").contains("did you mean"));
    }
}
