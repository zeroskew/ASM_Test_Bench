//! Differential test: every register / flag instruction of the emulator against the real CPU
//! running the tests (x86-64 executes the same 8 / 16 / 32-bit instructions). Each case runs
//! once on the hardware (inline assembly) and once in the emulator, with the same operands and
//! input flags; results and the flags the Intel / AMD manuals DEFINE for that instruction must
//! match. (Undefined flags, e.g. SF after mul, are not compared.)

#![cfg(all(test, target_arch = "x86_64"))]

use std::arch::asm;
use std::rc::Rc;

use crate::asm::{assemble, Severity};
use crate::cpu::{Machine, CF, OF, PF, SF, ZF};

const ALL: u32 = CF | PF | ZF | SF | OF;
const SZP: u32 = SF | ZF | PF;

// ---------------------------------------------------------------- test inputs

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }
    /// A random or an edge-case value.
    fn value(&mut self) -> u32 {
        const EDGES: [u32; 14] = [0, 1, 2, 3, 0x7F, 0x80, 0xFF, 0x7FFF, 0x8000, 0xFFFF, 0x7FFF_FFFF, 0x8000_0000, 0xFFFF_FFFF, 0xFFFF_FFFE];
        match self.next() % 4 {
            0 => EDGES[(self.next() as usize) % EDGES.len()],
            1 => self.next() & 0xFF,
            _ => self.next(),
        }
    }
    /// Input flags: a random mix of CF PF ZF SF OF (AF is not modelled: always 0).
    fn flags(&mut self) -> u32 {
        self.next() & ALL
    }
}

const CASES: usize = 400;
const SIZES: [(u32, &str, &str, &str); 3] = [(8, "al", "bl", "l"), (16, "ax", "bx", "x"), (32, "eax", "ebx", "e")];

fn mask(bits: u32) -> u32 {
    if bits == 32 { u32::MAX } else { (1 << bits) - 1 }
}

// ---------------------------------------------------------------- the emulator

/// Assembles `_start: <ins> / hlt` once; then runs it for many register / flag inputs.
struct Emu {
    prog: Rc<crate::asm::Program>,
}

impl Emu {
    fn new(ins: &str) -> Emu {
        let (p, d) = assemble(&format!("_start:\n{ins}\nhlt\n"));
        assert!(d.iter().all(|d| d.severity != Severity::Error), "{ins}: {d:#?}");
        Emu { prog: Rc::new(p) }
    }

    /// eax, ebx, ecx, edx in; (eax, ebx, edx, flags) out.
    fn run(&self, eax: u32, ebx: u32, ecx: u32, edx: u32, flags: u32) -> (u32, u32, u32, u32) {
        let mut m = Machine::new(self.prog.clone());
        m.regs[0] = eax;
        m.regs[3] = ebx;
        m.regs[1] = ecx;
        m.regs[2] = edx;
        m.eflags = 0x202 | flags;
        m.step();
        assert!(matches!(m.status, crate::cpu::Status::Ready), "faulted: {:?}", m.status);
        (m.regs[0], m.regs[3], m.regs[2], m.eflags)
    }
}

// ---------------------------------------------------------------- the hardware

/// Runs `$body` on the CPU with eax/ebx-like operands r (dst) and s (src), count in cl, the
/// input flags loaded with popfq; returns (r, s, flags).
macro_rules! hw {
    ($body:expr, $r:expr, $s:expr, $c:expr, $f:expr) => {{
        let (mut r, mut s, f_out): (u64, u64, u64);
        r = $r as u64;
        s = $s as u64;
        unsafe {
            asm!(
                "push {fi}",
                "popfq",
                $body,
                "/* {r} {s} */",
                "pushfq",
                "pop {fo}",
                r = inout(reg) r,
                s = inout(reg) s,
                fi = in(reg) (0x202u64 | $f as u64),
                fo = out(reg) f_out,
                in("cl") $c as u8,
            );
        }
        (r as u32, s as u32, f_out as u32)
    }};
}

/// Checks one two-operand (dst, src) instruction for all sizes.
fn check_binary(name: &str, flags_mask: u32, hw: &dyn Fn(u32, u32, u32, u32) -> (u32, u32, u32)) {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ name.len() as u64 * 7919);
    for (bits, dst, src, _) in SIZES {
        let emu = Emu::new(&format!("{name} {dst}, {src}"));
        for _ in 0..CASES {
            let (a, b, f) = (rng.value(), rng.value(), rng.flags());
            let (hr, hs, hf) = hw(bits, a, b, f);
            let (er, es, _, ef) = emu.run(a, b, 0, 0, f);
            let m = mask(bits);
            assert_eq!((er & m, es & m), (hr & m, hs & m), "{name} {bits}-bit: a={a:#X} b={b:#X} flags={f:#X}: result");
            assert_eq!(ef & flags_mask, hf & flags_mask, "{name} {bits}-bit: a={a:#X} b={b:#X} flags={f:#X}: flags (emu {ef:#X}, cpu {hf:#X})");
        }
    }
}

macro_rules! hw_sized {
    ($ins:literal, $bits:expr, $a:expr, $b:expr, $c:expr, $f:expr) => {
        match $bits {
            8 => hw!(concat!($ins, " {r:l}, {s:l}"), $a, $b, $c, $f),
            16 => hw!(concat!($ins, " {r:x}, {s:x}"), $a, $b, $c, $f),
            _ => hw!(concat!($ins, " {r:e}, {s:e}"), $a, $b, $c, $f),
        }
    };
}

macro_rules! check_bin {
    ($ins:literal, $mask:expr) => {
        check_binary($ins, $mask, &|bits, a, b, f| hw_sized!($ins, bits, a, b, 0, f))
    };
}

#[test]
fn arithmetic_and_logic() {
    check_bin!("add", ALL);
    check_bin!("adc", ALL);
    check_bin!("sub", ALL);
    check_bin!("sbb", ALL);
    check_bin!("cmp", ALL);
    check_bin!("and", ALL);
    check_bin!("or", ALL);
    check_bin!("xor", ALL);
    check_bin!("test", ALL);
    check_bin!("xchg", 0);
    check_bin!("xadd", ALL);
}

#[test]
fn unary() {
    let mut rng = Rng(42);
    macro_rules! unary {
        ($ins:literal, $mask:expr) => {{
            for (bits, dst, _, _) in SIZES {
                let emu = Emu::new(&format!("{} {dst}", $ins));
                for _ in 0..CASES {
                    let (a, f) = (rng.value(), rng.flags());
                    let (hr, _, hf) = match bits {
                        8 => hw!(concat!($ins, " {r:l}"), a, 0, 0, f),
                        16 => hw!(concat!($ins, " {r:x}"), a, 0, 0, f),
                        _ => hw!(concat!($ins, " {r:e}"), a, 0, 0, f),
                    };
                    let (er, _, _, ef) = emu.run(a, 0, 0, 0, f);
                    let m = mask(bits);
                    assert_eq!(er & m, hr & m, "{} {bits}-bit a={a:#X}", $ins);
                    assert_eq!(ef & $mask, hf & $mask, "{} {bits}-bit a={a:#X} flags={f:#X}", $ins);
                }
            }
        }};
    }
    unary!("inc", ALL); // (CF is kept: compared too)
    unary!("dec", ALL);
    unary!("neg", ALL);
    unary!("not", ALL); // changes no flags
    // bswap: 32-bit only.
    let emu = Emu::new("bswap eax");
    for _ in 0..CASES {
        let a = rng.value();
        let (hr, _, _) = hw!("bswap {r:e}", a, 0, 0, 0);
        assert_eq!(emu.run(a, 0, 0, 0, 0).0, hr);
    }
}

#[test]
fn shifts_and_rotates() {
    let mut rng = Rng(7);
    macro_rules! shift {
        ($ins:literal, $rot:expr, $through:expr) => {{
            for (bits, dst, _, _) in SIZES {
                let emu = Emu::new(&format!("{} {dst}, cl", $ins));
                for _ in 0..CASES {
                    let (a, f) = (rng.value(), rng.flags());
                    // Shifts: count 1..bits (bigger counts leave CF undefined for 8/16 bits).
                    // Rotates: any count 0..31.
                    let c = if $rot { rng.next() % 32 } else { 1 + rng.next() % bits };
                    let (hr, _, hf) = match bits {
                        8 => hw!(concat!($ins, " {r:l}, cl"), a, 0, c, f),
                        16 => hw!(concat!($ins, " {r:x}, cl"), a, 0, c, f),
                        _ => hw!(concat!($ins, " {r:e}, cl"), a, 0, c, f),
                    };
                    let (er, _, _, ef) = emu.run(a, 0, c, 0, f);
                    let m = mask(bits);
                    assert_eq!(er & m, hr & m, "{} {bits}-bit a={a:#X} count={c} flags={f:#X}", $ins);
                    // Defined flags: count 0 changes nothing; OF only for count 1; rotates
                    // don't touch SF ZF PF; shifts set SF ZF PF CF.
                    let eff = if $through { (c & 31) % (bits + 1) } else if $rot { (c & 31) % bits } else { c & 31 };
                    let raw = c & 31;
                    let mut fm = if raw == 0 { ALL } else if $rot { CF } else { CF | SZP };
                    if raw == 1 {
                        fm |= OF;
                    }
                    if $rot && !$through && eff == 0 && raw != 0 {
                        fm &= !CF; // rol/ror by a multiple of the size: CF is the low / high bit (not checked)
                    }
                    assert_eq!(ef & fm, hf & fm, "{} {bits}-bit a={a:#X} count={c} flags={f:#X}: flags (emu {ef:#X}, cpu {hf:#X})", $ins);
                }
            }
        }};
    }
    shift!("shl", false, false);
    shift!("shr", false, false);
    shift!("sar", false, false);
    shift!("rol", true, false);
    shift!("ror", true, false);
    shift!("rcl", true, true);
    shift!("rcr", true, true);
}

#[test]
fn double_shifts() {
    let mut rng = Rng(99);
    for ins in ["shld", "shrd"] {
        for (bits, dst, src, _) in SIZES.into_iter().skip(1) {
            let emu = Emu::new(&format!("{ins} {dst}, {src}, cl"));
            for _ in 0..CASES {
                let (a, b, f) = (rng.value(), rng.value(), rng.flags());
                let c = 1 + rng.next() % (bits - 1); // 1 .. bits-1 (more is undefined for 16-bit)
                let (hr, _, hf) = match (ins, bits) {
                    ("shld", 16) => hw!("shld {r:x}, {s:x}, cl", a, b, c, f),
                    ("shld", _) => hw!("shld {r:e}, {s:e}, cl", a, b, c, f),
                    (_, 16) => hw!("shrd {r:x}, {s:x}, cl", a, b, c, f),
                    _ => hw!("shrd {r:e}, {s:e}, cl", a, b, c, f),
                };
                let (er, _, _, ef) = emu.run(a, b, c, 0, f);
                let m = mask(bits);
                assert_eq!(er & m, hr & m, "{ins} {bits}-bit a={a:#X} b={b:#X} count={c}");
                let fm = CF | SZP | if c == 1 { OF } else { 0 };
                assert_eq!(ef & fm, hf & fm, "{ins} {bits}-bit a={a:#X} b={b:#X} count={c}: flags (emu {ef:#X}, cpu {hf:#X})");
            }
        }
    }
}

#[test]
fn multiply_and_divide() {
    let mut rng = Rng(1234);
    // mul / imul with one operand: AX = AL*src, DX:AX = AX*src, EDX:EAX = EAX*src.
    macro_rules! muldiv {
        ($ins:literal, $divide:expr, $signed:expr) => {{
            for (bits, _, src, m) in SIZES {
                let _ = m;
                let emu = Emu::new(&format!("{} {src}", $ins));
                let mut done = 0;
                while done < CASES {
                    let (a, b, d, f) = (rng.value(), rng.value(), rng.value(), rng.flags());
                    if $divide {
                        // Skip cases the CPU would fault on (division by zero / overflow).
                        let mk = mask(bits) as u64;
                        let (b0, lo, hi) = (b as u64 & mk, a as u64 & mk, d as u64 & mk);
                        if b0 == 0 {
                            continue;
                        }
                        let dividend = if bits == 8 { (a as u64) & 0xFFFF } else { (hi << bits) | lo };
                        let ok = if $signed {
                            let w = bits * 2;
                            let n = ((dividend << (64 - w)) as i64) >> (64 - w);
                            let v = ((b0 << (64 - bits)) as i64) >> (64 - bits);
                            let q = n / v;
                            q >= -(1i64 << (bits - 1)) && q < (1i64 << (bits - 1))
                        } else {
                            dividend / b0 <= mk
                        };
                        if !ok {
                            continue;
                        }
                    }
                    done += 1;
                    let (mut ha, mut hd, hf): (u64, u64, u64) = (a as u64, d as u64, 0);
                    let hf_ref = &mut 0u64;
                    unsafe {
                        match bits {
                            8 => asm!("push {fi}", "popfq", concat!($ins, " {s:l}"), "pushfq", "pop {fo}", s = in(reg) b as u64, fi = in(reg) (0x202 | f) as u64, fo = out(reg) *hf_ref, inout("rax") ha, inout("rdx") hd),
                            16 => asm!("push {fi}", "popfq", concat!($ins, " {s:x}"), "pushfq", "pop {fo}", s = in(reg) b as u64, fi = in(reg) (0x202 | f) as u64, fo = out(reg) *hf_ref, inout("rax") ha, inout("rdx") hd),
                            _ => asm!("push {fi}", "popfq", concat!($ins, " {s:e}"), "pushfq", "pop {fo}", s = in(reg) b as u64, fi = in(reg) (0x202 | f) as u64, fo = out(reg) *hf_ref, inout("rax") ha, inout("rdx") hd),
                        }
                    }
                    let _ = hf;
                    let hf = *hf_ref as u32;
                    let (ea, _, ed, ef) = emu.run(a, b, 0, d, f);
                    let (ha, hd) = (ha as u32, hd as u32);
                    match bits {
                        8 => assert_eq!(ea & 0xFFFF, ha & 0xFFFF, "{} 8-bit a={a:#X} b={b:#X}", $ins),
                        16 => assert_eq!((ea & 0xFFFF, ed & 0xFFFF), (ha & 0xFFFF, hd & 0xFFFF), "{} 16-bit a={a:#X} b={b:#X} d={d:#X}", $ins),
                        _ => assert_eq!((ea, ed), (ha, hd), "{} 32-bit a={a:#X} b={b:#X} d={d:#X}", $ins),
                    }
                    if !$divide {
                        assert_eq!(ef & (CF | OF), hf & (CF | OF), "{} {bits}-bit a={a:#X} b={b:#X}: CF/OF", $ins);
                    }
                }
            }
        }};
    }
    muldiv!("mul", false, false);
    muldiv!("imul", false, true);
    muldiv!("div", true, false);
    muldiv!("idiv", true, true);
    // imul with two and three operands (16 / 32 bits): CF and OF defined.
    for (bits, dst, src, _) in SIZES.into_iter().skip(1) {
        let two = Emu::new(&format!("imul {dst}, {src}"));
        let three = Emu::new(&format!("imul {dst}, {src}, -1000"));
        for _ in 0..CASES {
            let (a, b, f) = (rng.value(), rng.value(), rng.flags());
            let (hr, _, hf) = if bits == 16 { hw!("imul {r:x}, {s:x}", a, b, 0, f) } else { hw!("imul {r:e}, {s:e}", a, b, 0, f) };
            let (er, _, _, ef) = two.run(a, b, 0, 0, f);
            let m = mask(bits);
            assert_eq!((er & m, ef & (CF | OF)), (hr & m, hf & (CF | OF)), "imul {bits}-bit 2 operands a={a:#X} b={b:#X}");
            let (hr, _, hf) = if bits == 16 { hw!("imul {r:x}, {s:x}, -1000", a, b, 0, f) } else { hw!("imul {r:e}, {s:e}, -1000", a, b, 0, f) };
            let (er, _, _, ef) = three.run(a, b, 0, 0, f);
            assert_eq!((er & m, ef & (CF | OF)), (hr & m, hf & (CF | OF)), "imul {bits}-bit 3 operands b={b:#X}");
        }
    }
}

#[test]
fn bit_instructions() {
    let mut rng = Rng(555);
    for ins in ["bt", "bts", "btr", "btc", "bsf", "bsr", "popcnt"] {
        if ins == "popcnt" && !std::is_x86_feature_detected!("popcnt") {
            continue;
        }
        for (bits, dst, src, _) in SIZES.into_iter().skip(1) {
            let emu = Emu::new(&format!("{ins} {dst}, {src}"));
            for _ in 0..CASES {
                let (a, b, f) = (rng.value(), rng.value(), rng.flags());
                let (hr, _, hf) = match (ins, bits) {
                    ("bt", 16) => hw!("bt {r:x}, {s:x}", a, b, 0, f),
                    ("bt", _) => hw!("bt {r:e}, {s:e}", a, b, 0, f),
                    ("bts", 16) => hw!("bts {r:x}, {s:x}", a, b, 0, f),
                    ("bts", _) => hw!("bts {r:e}, {s:e}", a, b, 0, f),
                    ("btr", 16) => hw!("btr {r:x}, {s:x}", a, b, 0, f),
                    ("btr", _) => hw!("btr {r:e}, {s:e}", a, b, 0, f),
                    ("btc", 16) => hw!("btc {r:x}, {s:x}", a, b, 0, f),
                    ("btc", _) => hw!("btc {r:e}, {s:e}", a, b, 0, f),
                    ("bsf", 16) => hw!("bsf {r:x}, {s:x}", a, b, 0, f),
                    ("bsf", _) => hw!("bsf {r:e}, {s:e}", a, b, 0, f),
                    ("bsr", 16) => hw!("bsr {r:x}, {s:x}", a, b, 0, f),
                    ("bsr", _) => hw!("bsr {r:e}, {s:e}", a, b, 0, f),
                    ("popcnt", 16) => hw!("popcnt {r:x}, {s:x}", a, b, 0, f),
                    _ => hw!("popcnt {r:e}, {s:e}", a, b, 0, f),
                };
                let (er, _, _, ef) = emu.run(a, b, 0, 0, f);
                let m = mask(bits);
                let (fm, check_result) = match ins {
                    "bt" | "bts" | "btr" | "btc" => (CF, true),
                    "popcnt" => (ALL, true),
                    // bsf / bsr of 0: the destination is undefined (Intel); only ZF is defined.
                    _ => (ZF, b & m != 0),
                };
                if check_result {
                    assert_eq!(er & m, hr & m, "{ins} {bits}-bit a={a:#X} b={b:#X}: result");
                }
                assert_eq!(ef & fm, hf & fm, "{ins} {bits}-bit a={a:#X} b={b:#X}: flags (emu {ef:#X}, cpu {hf:#X})");
            }
        }
    }
}

#[test]
fn conversions_and_moves() {
    let mut rng = Rng(31337);
    let pairs: [(&str, u32); 4] = [("movzx eax, bl", 0), ("movsx eax, bl", 1), ("movzx eax, bx", 2), ("movsx eax, bx", 3)];
    for (src, k) in pairs {
        let emu = Emu::new(src);
        for _ in 0..CASES {
            let (a, b) = (rng.value(), rng.value());
            let (hr, _, _) = match k {
                0 => hw!("movzx {r:e}, {s:l}", a, b, 0, 0),
                1 => hw!("movsx {r:e}, {s:l}", a, b, 0, 0),
                2 => hw!("movzx {r:e}, {s:x}", a, b, 0, 0),
                _ => hw!("movsx {r:e}, {s:x}", a, b, 0, 0),
            };
            assert_eq!(emu.run(a, b, 0, 0, 0).0, hr, "{src} b={b:#X}");
        }
    }
    for (ins, k) in [("cbw", 0), ("cwde", 1), ("cwd", 2), ("cdq", 3)] {
        let emu = Emu::new(ins);
        for _ in 0..CASES {
            let (a, d) = (rng.value(), rng.value());
            let (mut ha, mut hd) = (a as u64, d as u64);
            unsafe {
                match k {
                    0 => asm!("cbw", inout("rax") ha, inout("rdx") hd),
                    1 => asm!("cwde", inout("rax") ha, inout("rdx") hd),
                    2 => asm!("cwd", inout("rax") ha, inout("rdx") hd),
                    _ => asm!("cdq", inout("rax") ha, inout("rdx") hd),
                }
            }
            let (ea, _, ed, _) = emu.run(a, 0, 0, d, 0);
            assert_eq!((ea, ed), (ha as u32, hd as u32), "{ins} a={a:#X}");
        }
    }
    // cmpxchg: compares EAX (AX / AL) with the destination.
    for (bits, _, src, _) in SIZES {
        let dst = ["cl", "cx", "ecx"][(bits / 16) as usize];
        let emu = Emu::new(&format!("cmpxchg {dst}, {src}"));
        for _ in 0..CASES {
            let (a, b, f) = (rng.value(), rng.value(), rng.flags());
            // Half the time the accumulator equals the destination.
            let c = if rng.next() % 2 == 0 { a } else { rng.value() };
            let (mut ha, mut hc, mut hf) = (a as u64, c as u64, 0u64);
            unsafe {
                match bits {
                    8 => asm!("push {fi}", "popfq", "cmpxchg cl, {s:l}", "pushfq", "pop {fo}", s = in(reg) b as u64, fi = in(reg) (0x202 | f) as u64, fo = out(reg) hf, inout("rax") ha, inout("rcx") hc),
                    16 => asm!("push {fi}", "popfq", "cmpxchg cx, {s:x}", "pushfq", "pop {fo}", s = in(reg) b as u64, fi = in(reg) (0x202 | f) as u64, fo = out(reg) hf, inout("rax") ha, inout("rcx") hc),
                    _ => asm!("push {fi}", "popfq", "cmpxchg ecx, {s:e}", "pushfq", "pop {fo}", s = in(reg) b as u64, fi = in(reg) (0x202 | f) as u64, fo = out(reg) hf, inout("rax") ha, inout("rcx") hc),
                }
            }
            let mut m = Machine::new(emu.prog.clone());
            m.regs[0] = a;
            m.regs[3] = b;
            m.regs[1] = c;
            m.eflags = 0x202 | f;
            m.step();
            let mk = mask(bits);
            assert_eq!((m.regs[0] & mk, m.regs[1] & mk), (ha as u32 & mk, hc as u32 & mk), "cmpxchg {bits}-bit a={a:#X} b={b:#X} c={c:#X}");
            assert_eq!(m.eflags & ALL, hf as u32 & ALL, "cmpxchg {bits}-bit flags");
        }
    }
}

#[test]
fn conditions_and_flag_transfers() {
    let mut rng = Rng(2024);
    // All 16 conditions, through setcc, for every combination of the five flags.
    macro_rules! setcc {
        ($($cc:literal),*) => {{
            $(
                let emu = Emu::new(concat!("set", $cc, " al"));
                for f in 0..32u32 {
                    let flags = (f & 1) * CF | ((f >> 1) & 1) * PF | ((f >> 2) & 1) * ZF | ((f >> 3) & 1) * SF | ((f >> 4) & 1) * OF;
                    let (hr, _, _) = hw!(concat!("set", $cc, " {r:l}"), 0xAA, 0, 0, flags);
                    let (er, _, _, _) = emu.run(0xAA, 0, 0, 0, flags);
                    assert_eq!(er & 0xFF, hr & 0xFF, "set{} with flags {flags:#X}", $cc);
                }
            )*
        }};
    }
    setcc!("o", "no", "b", "ae", "e", "ne", "be", "a", "s", "ns", "p", "np", "l", "ge", "le", "g");
    // lahf / sahf (AF is not modelled: kept 0 on both sides).
    let lahf = Emu::new("lahf");
    let sahf = Emu::new("sahf");
    for _ in 0..CASES {
        let (a, f) = (rng.value(), rng.flags());
        let (hr, _, _) = hw!("lahf", 0, 0, 0, f);
        let _ = hr;
        let (mut ha, mut hf) = (a as u64, 0u64);
        unsafe { asm!("push {fi}", "popfq", "lahf", "pushfq", "pop {fo}", fi = in(reg) (0x202 | f) as u64, fo = out(reg) hf, inout("rax") ha) };
        let _ = hf;
        let (ea, _, _, _) = lahf.run(a, 0, 0, 0, f);
        assert_eq!(ea, ha as u32, "lahf flags={f:#X}");
        let ah = (a & !0x1000) & 0xFFFF_EFFF; // AH without AF (bit 4 of AH)
        let (mut ha, mut hf) = (ah as u64, 0u64);
        unsafe { asm!("push {fi}", "popfq", "sahf", "pushfq", "pop {fo}", fi = in(reg) (0x202 | f) as u64, fo = out(reg) hf, inout("rax") ha) };
        let _ = ha;
        let (_, _, _, ef) = sahf.run(ah, 0, 0, 0, f);
        assert_eq!(ef & ALL, hf as u32 & ALL, "sahf ah={:#X}", (ah >> 8) & 0xFF);
    }
}

/// The same instructions with a memory operand give the same result as with registers.
#[test]
fn memory_forms_match_register_forms() {
    let forms: &[(&str, &str)] = &[
        ("add dword [v], ebx", "add eax, ebx"),
        ("adc word [v], bx", "adc ax, bx"),
        ("sbb byte [v], bl", "sbb al, bl"),
        ("xadd [v], ebx", "xadd eax, ebx"),
        ("rcl dword [v], cl", "rcl eax, cl"),
        ("rcr byte [v], 3", "rcr al, 3"),
        ("shld [v], ebx, cl", "shld eax, ebx, cl"),
        ("shrd word [v], bx, 5", "shrd ax, bx, 5"),
        ("btc dword [v], 7", "btc eax, 7"),
        ("bts word [v], 15", "bts ax, 15"),
        ("neg dword [v]", "neg eax"),
        ("not word [v]", "not ax"),
        ("sar dword [v], cl", "sar eax, cl"),
    ];
    let mut rng = Rng(77);
    for (mem_form, reg_form) in forms {
        let size = if mem_form.contains("byte") || mem_form.contains(" bl") { 1 } else if mem_form.contains("word [") && !mem_form.contains("dword") || mem_form.contains(" bx") { 2 } else { 4 };
        let (pm, d) = assemble(&format!("section .data\nv dd 0\nsection .text\n_start:\nmov [v], eax\n{mem_form}\nmov eax, [v]\nhlt\n"));
        assert!(d.is_empty(), "{mem_form}: {d:#?}");
        let (pr, d) = assemble(&format!("_start:\n{reg_form}\nhlt\n"));
        assert!(d.is_empty(), "{reg_form}: {d:#?}");
        for _ in 0..200 {
            let (a, b, c, f) = (rng.value(), rng.value(), rng.next() % 32, rng.flags());
            let run = |p: &crate::asm::Program, steps: usize| {
                let mut m = Machine::new(Rc::new(p.clone()));
                (m.regs[0], m.regs[3], m.regs[1]) = (a, b, c);
                m.eflags = 0x202 | f;
                for _ in 0..steps {
                    m.step();
                }
                (m.regs[0], m.eflags & ALL)
            };
            let mask = if size == 4 { u32::MAX } else { (1u32 << (size * 8)) - 1 };
            let (rm, fm) = run(&pm, 3);
            let (rr, fr) = run(&pr, 1);
            assert_eq!((rm & mask, fm), (rr & mask, fr), "{mem_form} vs {reg_form}: a={a:#X} b={b:#X} c={c}");
        }
    }
}
