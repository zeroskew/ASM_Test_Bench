//! The assembler: NASM-flavoured, Intel-syntax, 32-bit x86 subset.
//!
//! Source is turned into a [`Program`]: a list of decoded instructions (one per address slot in
//! the code segment), an initialised data image, and a symbol table. The emulator in
//! [`crate::cpu`] runs the decoded instructions directly, so there is no binary encoding step.

mod assembler;
mod expr;
mod lexer;

pub use assembler::{assemble, is_mnemonic};
pub use lexer::parse_number;

use std::collections::HashMap;

/// Memory layout: code at 0x1000, data + bss from 0x4000, the screen at 0xC000, and the
/// stack growing down from the top (0x20000). Small numbers keep addresses easy to read.
pub const CODE_BASE: u32 = 0x0000_1000;
/// Each instruction occupies one fixed-size slot (the emulator does not use real x86 encodings).
pub const INSTR_SIZE: u32 = 4;
pub const DATA_BASE: u32 = 0x0000_4000;
/// Memory-mapped screen: SCREEN_W x SCREEN_H bytes, one palette index per pixel.
pub const SCREEN_BASE: u32 = 0x0000_C000;
pub const SCREEN_W: u32 = 128;
pub const SCREEN_H: u32 = 128;
pub const SCREEN_END: u32 = SCREEN_BASE + SCREEN_W * SCREEN_H;
pub const MEM_SIZE: u32 = 0x0002_0000;
/// The stack grows down from the top of memory, and may not go below the screen.
pub const STACK_TOP: u32 = MEM_SIZE;
/// Return address pushed before the program starts: returning to it ends the program,
/// so `main: ... ret` works like a C main.
pub const EXIT_ADDR: u32 = 0x0000_0FFC;
pub const MAX_INSTRS: usize = ((DATA_BASE - CODE_BASE) / INSTR_SIZE) as usize;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Size {
    B,
    W,
    D,
}

impl Size {
    pub fn bytes(self) -> u32 {
        match self {
            Size::B => 1,
            Size::W => 2,
            Size::D => 4,
        }
    }
    pub fn bits(self) -> u32 {
        self.bytes() * 8
    }
    pub fn mask(self) -> u32 {
        match self {
            Size::B => 0xFF,
            Size::W => 0xFFFF,
            Size::D => 0xFFFF_FFFF,
        }
    }
    pub fn sign_bit(self) -> u32 {
        1 << (self.bits() - 1)
    }
    pub fn keyword(self) -> &'static str {
        match self {
            Size::B => "byte",
            Size::W => "word",
            Size::D => "dword",
        }
    }
    /// Sign-extends a value of this size to 32 bits.
    pub fn sext(self, v: u32) -> u32 {
        match self {
            Size::B => v as u8 as i8 as i32 as u32,
            Size::W => v as u16 as i16 as i32 as u32,
            Size::D => v,
        }
    }
}

/// A register operand. `idx` follows the x86 numbering (EAX ECX EDX EBX ESP EBP ESI EDI);
/// for byte registers 0..3 are AL CL DL BL and 4..7 are AH CH DH BH.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Reg {
    pub idx: u8,
    pub size: Size,
}

pub const REG32: [&str; 8] = ["eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi"];
pub const REG16: [&str; 8] = ["ax", "cx", "dx", "bx", "sp", "bp", "si", "di"];
pub const REG8: [&str; 8] = ["al", "cl", "dl", "bl", "ah", "ch", "dh", "bh"];

pub const EAX: u8 = 0;
pub const ECX: u8 = 1;
pub const EDX: u8 = 2;
pub const EBX: u8 = 3;
pub const ESP: u8 = 4;
pub const EBP: u8 = 5;
pub const ESI: u8 = 6;
pub const EDI: u8 = 7;

impl Reg {
    pub fn parse(name: &str) -> Option<Reg> {
        let n = name.to_ascii_lowercase();
        for (size, table) in [(Size::D, &REG32), (Size::W, &REG16), (Size::B, &REG8)] {
            if let Some(i) = table.iter().position(|r| *r == n) {
                return Some(Reg { idx: i as u8, size });
            }
        }
        None
    }
    pub fn name(self) -> &'static str {
        match self.size {
            Size::D => REG32[self.idx as usize],
            Size::W => REG16[self.idx as usize],
            Size::B => REG8[self.idx as usize],
        }
    }
    /// The 32-bit register this one is part of.
    pub fn full(self) -> u8 {
        if self.size == Size::B {
            self.idx & 3
        } else {
            self.idx
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Mem {
    pub base: Option<u8>,
    pub index: Option<u8>,
    pub scale: u8,
    pub disp: i32,
    /// Explicit size (`byte [x]`), if given.
    pub size: Option<Size>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Operand {
    Reg(Reg),
    Imm(u32),
    Mem(Mem),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Cond {
    O,
    No,
    B,
    Ae,
    E,
    Ne,
    Be,
    A,
    S,
    Ns,
    P,
    Np,
    L,
    Ge,
    Le,
    G,
}

impl Cond {
    /// Condition suffixes (as in `jXX`, `setXX`, `cmovXX`), including aliases.
    pub fn parse(s: &str) -> Option<Cond> {
        Some(match s {
            "o" => Cond::O,
            "no" => Cond::No,
            "b" | "c" | "nae" => Cond::B,
            "ae" | "nc" | "nb" => Cond::Ae,
            "e" | "z" => Cond::E,
            "ne" | "nz" => Cond::Ne,
            "be" | "na" => Cond::Be,
            "a" | "nbe" => Cond::A,
            "s" => Cond::S,
            "ns" => Cond::Ns,
            "p" | "pe" => Cond::P,
            "np" | "po" => Cond::Np,
            "l" | "nge" => Cond::L,
            "ge" | "nl" => Cond::Ge,
            "le" | "ng" => Cond::Le,
            "g" | "nle" => Cond::G,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Op {
    Mov,
    Movzx,
    Movsx,
    Lea,
    Xchg,
    Add,
    Adc,
    Sub,
    Sbb,
    Inc,
    Dec,
    Neg,
    Cmp,
    Mul,
    Imul,
    Div,
    Idiv,
    Cbw,
    Cwde,
    Cwd,
    Cdq,
    And,
    Or,
    Xor,
    Not,
    Test,
    Shl,
    Shr,
    Sar,
    Rol,
    Ror,
    Push,
    Pop,
    Pushad,
    Popad,
    Pushfd,
    Popfd,
    Call,
    Ret,
    Jmp,
    Jcc(Cond),
    Setcc(Cond),
    Cmovcc(Cond),
    Loop,
    Jecxz,
    Leave,
    Nop,
    Hlt,
    Int,
    Clc,
    Stc,
    Cmc,
    Cld,
    Std,
    /// String instructions; the size is the element size.
    Movs,
    Stos,
    Lods,
    Cmps,
    Scas,
    /// Rotate through carry.
    Rcl,
    Rcr,
    /// Bit test (and set / reset / complement).
    Bt,
    Bts,
    Btr,
    Btc,
    /// Bit scan forward / reverse, population count.
    Bsf,
    Bsr,
    Popcnt,
    /// Double-precision shifts.
    Shld,
    Shrd,
    Bswap,
    Xadd,
    Cmpxchg,
    Lahf,
    Sahf,
    /// loope / loopz, loopne / loopnz.
    Loope,
    Loopne,
    Xlat,
    Enter,
}

/// `rep` / `repe` / `repne` prefix on string instructions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rep {
    None,
    Rep,
    Repe,
    Repne,
}

#[derive(Clone, Debug)]
pub struct Instr {
    pub op: Op,
    pub operands: Vec<Operand>,
    /// Operation size (resolved from registers or size keywords).
    pub size: Size,
    pub rep: Rep,
    /// 0-based source line.
    pub line: usize,
    pub addr: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SymKind {
    Code,
    Data,
    Bss,
    Const,
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub value: u32,
    pub kind: SymKind,
    pub line: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub line: usize,
    /// Character columns (start, end) on the line, for underlining.
    pub cols: (usize, usize),
    pub message: String,
    pub severity: Severity,
}

#[derive(Clone, Debug, Default)]
pub struct Program {
    pub instrs: Vec<Instr>,
    /// Initialised data, loaded at [`DATA_BASE`].
    pub data: Vec<u8>,
    pub bss_base: u32,
    pub bss_size: u32,
    pub entry: u32,
    pub symbols: Vec<Symbol>,
    /// Source line -> address of the instruction on it.
    pub line_addr: HashMap<usize, u32>,
    /// Source line -> (address, length) of the data it defines.
    #[allow(dead_code)]
    pub data_lines: HashMap<usize, (u32, u32)>,
}

impl Program {
    pub fn instr_at(&self, addr: u32) -> Option<&Instr> {
        if addr < CODE_BASE || (addr - CODE_BASE) % INSTR_SIZE != 0 {
            return None;
        }
        self.instrs.get(((addr - CODE_BASE) / INSTR_SIZE) as usize)
    }
    pub fn code_end(&self) -> u32 {
        CODE_BASE + self.instrs.len() as u32 * INSTR_SIZE
    }
    /// End of static data (data + bss); the stack must not grow below this.
    pub fn static_end(&self) -> u32 {
        self.bss_base + self.bss_size
    }
    pub fn symbol(&self, name: &str) -> Option<&Symbol> {
        self.symbols.iter().find(|s| s.name.eq_ignore_ascii_case(name))
    }
    /// The label at exactly this code address, if any.
    pub fn label_at(&self, addr: u32) -> Option<&str> {
        self.symbols.iter().find(|s| s.kind == SymKind::Code && s.value == addr && !s.name.contains('.')).or_else(|| self.symbols.iter().find(|s| s.kind == SymKind::Code && s.value == addr)).map(|s| s.name.as_str())
    }
    /// Nearest symbol at or below `addr` in the data / bss area, with the offset into it.
    pub fn data_symbol_for(&self, addr: u32) -> Option<(&str, u32)> {
        self.symbols
            .iter()
            .filter(|s| matches!(s.kind, SymKind::Data | SymKind::Bss) && s.value <= addr)
            .max_by_key(|s| s.value)
            .map(|s| (s.name.as_str(), addr - s.value))
    }
    /// Name of the function containing a code address: the closest non-local label at or below.
    pub fn function_for(&self, addr: u32) -> Option<&str> {
        self.symbols
            .iter()
            .filter(|s| s.kind == SymKind::Code && s.value <= addr && !s.name.contains('.'))
            .max_by_key(|s| s.value)
            .map(|s| s.name.as_str())
    }
}

/// Text form of an instruction (used by the disassembly view and tooltips).
pub fn format_instr(i: &Instr, prog: &Program) -> String {
    let mut s = String::new();
    match i.rep {
        Rep::Rep => s.push_str("rep "),
        Rep::Repe => s.push_str("repe "),
        Rep::Repne => s.push_str("repne "),
        Rep::None => {}
    }
    s.push_str(&mnemonic(i));
    let branch = matches!(i.op, Op::Jmp | Op::Jcc(_) | Op::Call | Op::Loop | Op::Loope | Op::Loopne | Op::Jecxz);
    for (n, o) in i.operands.iter().enumerate() {
        s.push_str(if n == 0 { " " } else { ", " });
        match o {
            Operand::Reg(r) => s.push_str(r.name()),
            Operand::Imm(v) => {
                if branch {
                    match prog.label_at(*v) {
                        Some(l) => s.push_str(l),
                        None => s.push_str(&format!("0x{v:X}")),
                    }
                } else if *v < 10 || (*v as i32) > -10 && (*v as i32) < 0 {
                    s.push_str(&format!("{}", *v as i32));
                } else {
                    s.push_str(&format!("0x{v:X}"));
                }
            }
            Operand::Mem(m) => {
                if let Some(sz) = m.size {
                    s.push_str(sz.keyword());
                    s.push(' ');
                }
                s.push('[');
                let mut first = true;
                if let Some(b) = m.base {
                    s.push_str(REG32[b as usize]);
                    first = false;
                }
                if let Some(x) = m.index {
                    if !first {
                        s.push('+');
                    }
                    s.push_str(REG32[x as usize]);
                    if m.scale != 1 {
                        s.push_str(&format!("*{}", m.scale));
                    }
                    first = false;
                }
                if m.disp != 0 || first {
                    let named = if m.disp > 0 { prog.data_symbol_for(m.disp as u32).filter(|(_, off)| *off == 0).map(|(n, _)| n) } else { None };
                    match named {
                        Some(n) => {
                            if !first {
                                s.push('+');
                            }
                            s.push_str(n);
                        }
                        None if first => s.push_str(&format!("0x{:X}", m.disp as u32)),
                        None if m.disp < 0 => s.push_str(&format!("-{}", -(m.disp as i64))),
                        None => s.push_str(&format!("+{}", m.disp)),
                    }
                }
                s.push(']');
            }
        }
    }
    s
}

pub fn mnemonic(i: &Instr) -> String {
    let cc = |c: Cond| format!("{c:?}").to_ascii_lowercase();
    let sfx = match i.size {
        Size::B => "b",
        Size::W => "w",
        Size::D => "d",
    };
    match i.op {
        Op::Jcc(c) => format!("j{}", cc(c)),
        Op::Setcc(c) => format!("set{}", cc(c)),
        Op::Cmovcc(c) => format!("cmov{}", cc(c)),
        Op::Movs | Op::Stos | Op::Lods | Op::Cmps | Op::Scas => format!("{}{}", format!("{:?}", i.op).to_ascii_lowercase(), sfx),
        op => format!("{op:?}").to_ascii_lowercase(),
    }
}

/// Constants every program can use without defining them.
pub const BUILTINS: &[(&str, u32, &str)] = &[
    ("SCREEN", SCREEN_BASE, "address of the screen: pixel (x, y) is the byte at SCREEN + y*SCREEN_W + x"),
    ("SCREEN_W", SCREEN_W, "screen width in pixels"),
    ("SCREEN_H", SCREEN_H, "screen height in pixels"),
    // Key codes returned by read_key (210-217 system calls); letters are upper-case ASCII.
    ("KEY_LEFT", 128, "key code of the Left arrow"),
    ("KEY_RIGHT", 129, "key code of the Right arrow"),
    ("KEY_UP", 130, "key code of the Up arrow"),
    ("KEY_DOWN", 131, "key code of the Down arrow"),
    ("KEY_SPACE", 32, "key code of Space"),
    ("KEY_ENTER", 13, "key code of Enter"),
    ("KEY_ESC", 27, "key code of Escape"),
    // A few palette colours (the palette is xterm-256: 0-15 basic, 16-231 a 6x6x6 cube, 232-255 greys).
    ("BLACK", 0, "palette colour 0"),
    ("GRAY", 8, "palette colour 8"),
    ("RED", 9, "palette colour 9"),
    ("GREEN", 10, "palette colour 10"),
    ("YELLOW", 11, "palette colour 11"),
    ("BLUE", 12, "palette colour 12"),
    ("MAGENTA", 13, "palette colour 13"),
    ("CYAN", 14, "palette colour 14"),
    ("WHITE", 15, "palette colour 15"),
    // Files (open / lseek system calls, Linux values).
    ("O_RDONLY", 0, "open: read only"),
    ("O_WRONLY", 1, "open: write only"),
    ("O_RDWR", 2, "open: read and write"),
    ("O_CREAT", 0x40, "open: create the file if it doesn't exist"),
    ("O_EXCL", 0x80, "open (with O_CREAT): fail if the file already exists"),
    ("O_TRUNC", 0x200, "open: empty the file first"),
    ("O_APPEND", 0x400, "open: every write goes to the end of the file"),
    ("SEEK_SET", 0, "lseek: from the start of the file"),
    ("SEEK_CUR", 1, "lseek: from the current position"),
    ("SEEK_END", 2, "lseek: from the end of the file"),
    ("STDIN", 0, "file descriptor of the keyboard / console input"),
    ("STDOUT", 1, "file descriptor of the console output"),
    ("STDERR", 2, "file descriptor of the console (errors)"),
];

pub fn builtin(name: &str) -> Option<(u32, &'static str)> {
    BUILTINS.iter().find(|(n, _, _)| *n == name).map(|(_, v, d)| (*v, *d))
}

/// The screen palette (xterm-256): 0-15 the classic 16 colours, 16-231 a 6x6x6 RGB cube
/// (index = 16 + 36*r + 6*g + b, each 0..5), 232-255 a grey ramp.
pub fn palette(i: u8) -> [u8; 3] {
    const BASIC: [[u8; 3]; 16] = [
        [0, 0, 0], [128, 0, 0], [0, 128, 0], [128, 128, 0], [0, 0, 128], [128, 0, 128], [0, 128, 128], [192, 192, 192],
        [128, 128, 128], [255, 0, 0], [0, 255, 0], [255, 255, 0], [0, 0, 255], [255, 0, 255], [0, 255, 255], [255, 255, 255],
    ];
    match i {
        0..=15 => BASIC[i as usize],
        16..=231 => {
            let n = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
            [level(n / 36), level((n / 6) % 6), level(n % 6)]
        }
        _ => {
            let g = 8 + 10 * (i - 232);
            [g, g, g]
        }
    }
}
