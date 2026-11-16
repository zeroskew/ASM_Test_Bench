//! The emulated 32-bit x86 machine.
//!
//! Every step records an undo entry (old registers, overwritten memory bytes, console output
//! length, consumed input), so the debugger can step backwards.

use std::collections::VecDeque;
use std::rc::Rc;

use crate::asm::*;

pub const CF: u32 = 1 << 0;
pub const PF: u32 = 1 << 2;
pub const ZF: u32 = 1 << 6;
pub const SF: u32 = 1 << 7;
pub const DF: u32 = 1 << 10;
pub const OF: u32 = 1 << 11;
/// Bit 1 of EFLAGS is always set; bit 9 (IF) is set as on a real machine.
const FLAGS_FIXED: u32 = 0x202;

/// Most steps kept for stepping backwards.
pub const HISTORY_LIMIT: usize = 200_000;
/// And at most this much memory (screen-heavy programs record many bytes per step).
pub const HISTORY_BYTES: usize = 64 << 20;
const OUTPUT_LIMIT: usize = 4 << 20;

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Can execute.
    Ready,
    /// Program ended (exit code).
    Exited(i32),
    /// Stopped by an error; EIP points at the faulting instruction.
    Fault(String),
    /// A read system call is waiting for a line of console input.
    WaitInput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepEvent {
    Ok,
    /// `int3` was executed (a breakpoint written in the code).
    Int3,
    /// The program asked to wait for the next display frame (animation).
    Frame,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    /// Address of the called function.
    pub target: u32,
    /// Where the call came from.
    pub call_site: u32,
    pub ret_addr: u32,
    /// ESP right after the call pushed the return address.
    pub esp: u32,
}

#[derive(Clone, Default)]
struct Undo {
    regs: [u32; 8],
    eip: u32,
    eflags: u32,
    mem: Vec<(u32, u8)>,
    out_len: usize,
    input_taken: Vec<u8>,
    frames: Option<Vec<Frame>>,
    exited: bool,
    prev_regs: [u32; 8],
    prev_eflags: u32,
    prev_eip: u32,
    last_writes: Vec<(u32, u32)>,
    counted: Option<usize>,
    ticks: u64,
    rng: u64,
    keys_taken: Vec<u32>,
    brk: u32,
    /// The step changed something outside the machine (a file): history can't go back past it.
    external: bool,
}

impl Undo {
    /// Approximate heap + inline size, for the history memory cap.
    fn cost(&self) -> usize {
        std::mem::size_of::<Undo>()
            + self.mem.len() * 8
            + self.input_taken.len()
            + self.keys_taken.len() * 4
            + self.last_writes.len() * 8
            + self.frames.as_ref().map_or(0, |f| f.len() * std::mem::size_of::<Frame>())
    }
}

pub struct Machine {
    pub prog: Rc<Program>,
    pub regs: [u32; 8],
    pub eip: u32,
    pub eflags: u32,
    pub mem: Vec<u8>,
    pub status: Status,
    pub steps: u64,
    pub output: String,
    pub input: VecDeque<u8>,
    pub frames: Vec<Frame>,
    /// Register values before the last step (to highlight changes).
    pub prev_regs: [u32; 8],
    pub prev_eflags: u32,
    /// Address of the last executed instruction.
    pub prev_eip: u32,
    /// Memory ranges written by the last step.
    pub last_writes: Vec<(u32, u32)>,
    /// Times each instruction was executed.
    pub hits: Vec<u64>,
    /// Op of the last executed instruction.
    pub last_op: Option<Op>,
    /// Frames waited for with the wait_frame call (the "clock" for animations).
    pub ticks: u64,
    rng: u64,
    /// Keys pressed in the Screen window, oldest first (read_key consumes them).
    pub keys: VecDeque<u32>,
    /// Keys currently held down (indexed by key code).
    pub keys_down: [bool; 256],
    /// Set once the program writes to screen memory (the UI then shows the Screen).
    pub screen_used: bool,
    /// The program's files (open / read / write / ... system calls), inside one folder.
    pub files: crate::files::Sandbox,
    /// The program break (end of the heap, brk system call): starts after .data / .bss.
    pub brk: u32,
    brk_start: u32,
    /// Set by the wait_frame call during a step.
    frame_wait: bool,
    history: VecDeque<Undo>,
    /// Approximate memory used by `history`.
    history_bytes: usize,
    cur: Undo,
}

type R<T> = Result<T, String>;

impl Machine {
    pub fn new(prog: Rc<Program>) -> Self {
        let mut mem = vec![0u8; MEM_SIZE as usize];
        mem[DATA_BASE as usize..DATA_BASE as usize + prog.data.len()].copy_from_slice(&prog.data);
        let mut regs = [0u32; 8];
        // The program is "called" by the loader: returning from main ends it.
        regs[ESP as usize] = STACK_TOP - 4;
        mem[(STACK_TOP - 4) as usize..STACK_TOP as usize].copy_from_slice(&EXIT_ADDR.to_le_bytes());
        let hits = vec![0; prog.instrs.len()];
        let entry = prog.entry;
        let brk = (prog.static_end() + 15) & !15;
        Machine {
            prog,
            regs,
            eip: entry,
            eflags: FLAGS_FIXED,
            mem,
            status: Status::Ready,
            steps: 0,
            output: String::new(),
            input: VecDeque::new(),
            frames: Vec::new(),
            prev_regs: regs,
            prev_eflags: FLAGS_FIXED,
            prev_eip: entry,
            last_writes: Vec::new(),
            hits,
            last_op: None,
            ticks: 0,
            rng: 0x2545_F491_4F6C_DD1D,
            keys: VecDeque::new(),
            keys_down: [false; 256],
            screen_used: false,
            frame_wait: false,
            files: crate::files::Sandbox::new(None),
            brk,
            brk_start: brk,
            history: VecDeque::new(),
            history_bytes: 0,
            cur: Undo::default(),
        }
    }

    pub fn can_step_back(&self) -> bool {
        !self.history.is_empty() || matches!(self.status, Status::Fault(_))
    }

    pub fn current_instr(&self) -> Option<&Instr> {
        self.prog.instr_at(self.eip)
    }

    pub fn flag(&self, f: u32) -> bool {
        self.eflags & f != 0
    }

    fn set_flag(&mut self, f: u32, on: bool) {
        if on {
            self.eflags |= f;
        } else {
            self.eflags &= !f;
        }
    }

    pub fn reg(&self, r: Reg) -> u32 {
        let v = self.regs[r.full() as usize];
        match r.size {
            Size::D => v,
            Size::W => v & 0xFFFF,
            Size::B if r.idx < 4 => v & 0xFF,
            Size::B => (v >> 8) & 0xFF,
        }
    }

    fn set_reg(&mut self, r: Reg, v: u32) {
        let full = &mut self.regs[r.full() as usize];
        *full = match r.size {
            Size::D => v,
            Size::W => (*full & 0xFFFF_0000) | (v & 0xFFFF),
            Size::B if r.idx < 4 => (*full & 0xFFFF_FF00) | (v & 0xFF),
            Size::B => (*full & 0xFFFF_00FF) | ((v & 0xFF) << 8),
        };
    }

    /// Reads memory for display (no checks, no side effects).
    pub fn peek(&self, addr: u32, size: u32) -> Option<u32> {
        let a = addr as usize;
        if addr.checked_add(size)? > MEM_SIZE {
            return None;
        }
        let mut b = [0u8; 4];
        b[..size as usize].copy_from_slice(&self.mem[a..a + size as usize]);
        Some(u32::from_le_bytes(b))
    }

    fn check(&self, addr: u32, size: u32, write: bool) -> R<()> {
        let what = if write { "writing" } else { "reading" };
        let end = addr as u64 + size as u64;
        if addr < CODE_BASE {
            return Err(format!("Access violation {what} address 0x{addr:08X} (a null or uninitialised pointer?)"));
        }
        if end > MEM_SIZE as u64 {
            return Err(format!("Access violation {what} address 0x{addr:08X}: outside memory (0x{CODE_BASE:X}..0x{MEM_SIZE:X})"));
        }
        if addr < DATA_BASE {
            if write {
                return Err(format!("Access violation writing 0x{addr:08X}: the code segment is read-only"));
            }
            return Err(format!("Access violation reading 0x{addr:08X}: that's in the code segment, not data"));
        }
        Ok(())
    }

    fn read(&mut self, addr: u32, size: Size) -> R<u32> {
        self.check(addr, size.bytes(), false)?;
        Ok(self.peek(addr, size.bytes()).unwrap())
    }

    fn write(&mut self, addr: u32, size: Size, v: u32) -> R<()> {
        self.check(addr, size.bytes(), true)?;
        let bytes = v.to_le_bytes();
        for i in 0..size.bytes() {
            let a = (addr + i) as usize;
            self.cur.mem.push((addr + i, self.mem[a]));
            self.mem[a] = bytes[i as usize];
        }
        if addr < SCREEN_END && addr + size.bytes() > SCREEN_BASE {
            self.screen_used = true;
        }
        match self.last_writes.last_mut() {
            Some(w) if w.0 + w.1 == addr => w.1 += size.bytes(),
            _ => self.last_writes.push((addr, size.bytes())),
        }
        Ok(())
    }

    pub fn ea(&self, m: &Mem) -> u32 {
        let mut a = m.disp as u32;
        if let Some(b) = m.base {
            a = a.wrapping_add(self.regs[b as usize]);
        }
        if let Some(i) = m.index {
            a = a.wrapping_add(self.regs[i as usize].wrapping_mul(m.scale as u32));
        }
        a
    }

    fn get(&mut self, o: &Operand, size: Size) -> R<u32> {
        match o {
            Operand::Reg(r) => Ok(self.reg(*r)),
            Operand::Imm(v) => Ok(v & size.mask()),
            Operand::Mem(m) => {
                let a = self.ea(m);
                self.read(a, size)
            }
        }
    }

    fn set(&mut self, o: &Operand, size: Size, v: u32) -> R<()> {
        match o {
            Operand::Reg(r) => {
                self.set_reg(*r, v);
                Ok(())
            }
            Operand::Mem(m) => {
                let a = self.ea(m);
                self.write(a, size, v & size.mask())
            }
            Operand::Imm(_) => Err("can't write to a constant".into()),
        }
    }

    fn push(&mut self, size: Size, v: u32) -> R<()> {
        let esp = self.regs[ESP as usize].wrapping_sub(size.bytes());
        if esp < SCREEN_END {
            return Err(format!("Stack overflow: ESP would go down to 0x{esp:08X}, into the screen memory below the stack (runaway recursion, or more pushes than pops?)"));
        }
        self.write(esp, size, v)?;
        self.regs[ESP as usize] = esp;
        Ok(())
    }

    fn pop(&mut self, size: Size) -> R<u32> {
        let esp = self.regs[ESP as usize];
        if esp as u64 + size.bytes() as u64 > STACK_TOP as u64 {
            return Err("Stack underflow: pop/ret with an empty stack (more pops than pushes?)".into());
        }
        let v = self.read(esp, size)?;
        self.regs[ESP as usize] = esp + size.bytes();
        Ok(v)
    }

    fn szp(&mut self, r: u32, s: Size) {
        let r = r & s.mask();
        self.set_flag(ZF, r == 0);
        self.set_flag(SF, r & s.sign_bit() != 0);
        self.set_flag(PF, (r as u8).count_ones() % 2 == 0);
    }

    fn add(&mut self, a: u32, b: u32, carry: u32, s: Size) -> u32 {
        let full = a as u64 + b as u64 + carry as u64;
        let r = (full as u32) & s.mask();
        self.set_flag(CF, full > s.mask() as u64);
        self.set_flag(OF, ((a ^ r) & (b ^ r) & s.sign_bit()) != 0);
        self.szp(r, s);
        r
    }

    fn sub(&mut self, a: u32, b: u32, borrow: u32, s: Size) -> u32 {
        let r = a.wrapping_sub(b).wrapping_sub(borrow) & s.mask();
        self.set_flag(CF, (a as u64) < b as u64 + borrow as u64);
        self.set_flag(OF, ((a ^ b) & (a ^ r) & s.sign_bit()) != 0);
        self.szp(r, s);
        r
    }

    fn logic(&mut self, r: u32, s: Size) -> u32 {
        self.set_flag(CF, false);
        self.set_flag(OF, false);
        self.szp(r, s);
        r & s.mask()
    }

    pub fn cond(&self, c: Cond) -> bool {
        let (cf, zf, sf, of, pf) = (self.flag(CF), self.flag(ZF), self.flag(SF), self.flag(OF), self.flag(PF));
        match c {
            Cond::O => of,
            Cond::No => !of,
            Cond::B => cf,
            Cond::Ae => !cf,
            Cond::E => zf,
            Cond::Ne => !zf,
            Cond::Be => cf || zf,
            Cond::A => !cf && !zf,
            Cond::S => sf,
            Cond::Ns => !sf,
            Cond::P => pf,
            Cond::Np => !pf,
            Cond::L => sf != of,
            Cond::Ge => sf == of,
            Cond::Le => zf || sf != of,
            Cond::G => !zf && sf == of,
        }
    }

    fn out(&mut self, s: &str) -> R<()> {
        if self.output.len() + s.len() > OUTPUT_LIMIT {
            return Err("Console output limit (4 MB) reached; is the program stuck in a printing loop?".into());
        }
        self.output.push_str(s);
        Ok(())
    }

    /// Takes one line from the input buffer (including '\n'), or None if no full line yet.
    fn take_line(&mut self) -> Option<Vec<u8>> {
        let nl = self.input.iter().position(|&b| b == b'\n')?;
        let line: Vec<u8> = self.input.drain(..=nl).collect();
        self.cur.input_taken.extend_from_slice(&line);
        Some(line)
    }

    /// A Linux-style result in EAX: the value, or -errno on failure.
    fn sys_result(&mut self, r: Result<u32, i32>) {
        self.regs[EAX as usize] = match r {
            Ok(v) => v,
            Err(e) => (-e) as u32,
        };
    }

    fn read_cstr(&mut self, mut addr: u32) -> R<Vec<u8>> {
        let mut v = Vec::new();
        loop {
            let b = self.read(addr, Size::B)? as u8;
            if b == 0 {
                return Ok(v);
            }
            v.push(b);
            if v.len() > 65536 {
                return Err("string has no terminating 0 byte".into());
            }
            addr += 1;
        }
    }

    /// Returns Ok(false) when the call has to wait for input.
    fn syscall(&mut self) -> R<bool> {
        let eax = self.regs[EAX as usize];
        let (ebx, ecx, edx) = (self.regs[EBX as usize], self.regs[ECX as usize], self.regs[EDX as usize]);
        match eax {
            1 => {
                self.status = Status::Exited(ebx as i32);
                self.cur.exited = true;
            }
            3 if ebx >= 3 => {
                // read(fd, buf, count) from a file
                self.cur.external = true;
                let r = self.files.read(ebx, edx as usize);
                let v = match r {
                    Ok(bytes) => {
                        for (i, &b) in bytes.iter().enumerate() {
                            self.write(ecx.wrapping_add(i as u32), Size::B, b as u32)?;
                        }
                        Ok(bytes.len() as u32)
                    }
                    Err(e) => Err(e),
                };
                self.sys_result(v);
            }
            4 if ebx >= 3 => {
                // write(fd, buf, count) to a file
                if edx as usize > crate::files::MAX_IO {
                    return Err(format!("write: length {edx} is too large (did you forget to set EDX?)"));
                }
                let mut bytes = Vec::with_capacity(edx as usize);
                for i in 0..edx {
                    bytes.push(self.read(ecx.wrapping_add(i), Size::B)? as u8);
                }
                self.cur.external = true;
                let v = self.files.write(ebx, &bytes).map(|n| n as u32);
                self.sys_result(v);
            }
            5 | 8 => {
                // open(path, flags, mode) / creat(path, mode)
                let path = self.read_cstr(ebx)?;
                let flags = if eax == 8 { crate::files::O_CREAT | crate::files::O_WRONLY | crate::files::O_TRUNC } else { ecx };
                self.cur.external = true;
                let v = self.files.open(&path, flags);
                self.sys_result(v);
            }
            6 => {
                self.cur.external = ebx >= 3;
                let v = self.files.close(ebx).map(|_| 0);
                self.sys_result(v);
            }
            10 => {
                let path = self.read_cstr(ebx)?;
                self.cur.external = true;
                let v = self.files.unlink(&path).map(|_| 0);
                self.sys_result(v);
            }
            13 => {
                // time(tloc): seconds since 1970
                let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as u32).unwrap_or(0);
                if ebx != 0 {
                    self.write(ebx, Size::D, t)?;
                }
                self.regs[EAX as usize] = t;
            }
            19 => {
                self.cur.external = true;
                let v = self.files.lseek(ebx, ecx as i32, edx);
                self.sys_result(v);
            }
            20 => self.regs[EAX as usize] = 1, // getpid: the only process
            38 => {
                let (from, to) = (self.read_cstr(ebx)?, self.read_cstr(ecx)?);
                self.cur.external = true;
                let v = self.files.rename(&from, &to).map(|_| 0);
                self.sys_result(v);
            }
            39 | 40 => {
                let path = self.read_cstr(ebx)?;
                self.cur.external = true;
                let v = if eax == 39 { self.files.mkdir(&path) } else { self.files.rmdir(&path) }.map(|_| 0);
                self.sys_result(v);
            }
            45 => {
                // brk(addr): grows / shrinks the heap after .data / .bss, up to the screen
                // memory. brk(0) (or a bad address) returns the current break.
                if ebx >= self.brk_start && ebx <= SCREEN_BASE {
                    self.brk = ebx;
                }
                self.regs[EAX as usize] = self.brk;
            }
            3 => {
                if ebx != 0 {
                    return Err(format!("read: file descriptor {ebx} is not open for reading (0 = the console; open a file first)"));
                }
                let Some(nl) = self.input.iter().position(|&b| b == b'\n') else { return Ok(false) };
                let n = (nl as u32 + 1).min(edx) as usize;
                let bytes: Vec<u8> = self.input.drain(..n).collect();
                self.cur.input_taken.extend_from_slice(&bytes);
                for (i, &b) in bytes.iter().enumerate() {
                    self.write(ecx.wrapping_add(i as u32), Size::B, b as u32)?;
                }
                self.regs[EAX as usize] = n as u32;
            }
            4 => {
                if ebx != 1 && ebx != 2 {
                    return Err("write: file descriptor 0 is the keyboard; write to 1 (the console) or to a file opened with open".into());
                }
                if edx > 1 << 20 {
                    return Err(format!("write: length {edx} is too large (did you forget to set EDX?)"));
                }
                let mut bytes = Vec::with_capacity(edx as usize);
                for i in 0..edx {
                    bytes.push(self.read(ecx.wrapping_add(i), Size::B)? as u8);
                }
                let s = String::from_utf8_lossy(&bytes).into_owned();
                self.out(&s)?;
                self.regs[EAX as usize] = edx;
            }
            // ---- ASM Test Bench helpers (not Linux) ----
            200 => self.out(&format!("{}", ebx as i32))?,
            201 => {
                let c = (ebx & 0xFF) as u8;
                self.out(&String::from_utf8_lossy(&[c]))?;
            }
            202 => {
                let s = self.read_cstr(ebx)?;
                self.out(&String::from_utf8_lossy(&s))?;
            }
            203 => {
                let Some(line) = self.take_line() else { return Ok(false) };
                let text = String::from_utf8_lossy(&line).trim().to_string();
                let v: i64 = text.parse().map_err(|_| format!("read_int: '{text}' is not a number"))?;
                self.regs[EAX as usize] = v as i32 as u32;
            }
            204 => self.out(&format!("0x{ebx:08X}"))?,
            205 => self.out("\n")?,
            206 => self.out(&format!("{ebx}"))?,
            // ---- Screen, keyboard, time (ASM Test Bench) ----
            210 => self.fill_rect(0, 0, SCREEN_W as i32, SCREEN_H as i32, ebx)?,
            211 => {
                // set_pixel(x, y, color): off-screen pixels are ignored.
                if ebx < SCREEN_W && ecx < SCREEN_H {
                    self.write(SCREEN_BASE + ecx * SCREEN_W + ebx, Size::B, edx & 0xFF)?;
                }
            }
            212 => {
                let (esi, edi) = (self.regs[ESI as usize], self.regs[EDI as usize]);
                self.fill_rect(ebx as i32, ecx as i32, edx as i32, esi as i32, edi)?;
            }
            213 => {
                self.ticks += 1;
                self.frame_wait = true;
            }
            214 => {
                let k = self.keys.pop_front().unwrap_or(0);
                if k != 0 {
                    self.cur.keys_taken.push(k);
                }
                self.regs[EAX as usize] = k;
            }
            215 => self.regs[EAX as usize] = self.keys_down.get(ebx as usize).copied().unwrap_or(false) as u32,
            216 => {
                let r = self.next_random();
                self.regs[EAX as usize] = if ebx == 0 { r } else { r % ebx };
            }
            217 => self.regs[EAX as usize] = self.ticks as u32,
            _ => return Err(format!("Unknown system call EAX={eax}. Supported: 1 exit, 3 read, 4 write, 5 open, 6 close, 8 creat, 10 unlink, 13 time, 19 lseek, 20 getpid, 38 rename, 39 mkdir, 40 rmdir, 45 brk, and the ASM Test Bench helpers 200-206 and 210-217 (see the Reference tab)")),
        }
        Ok(true)
    }

    fn begin_undo(&mut self) {
        self.cur = Undo {
            regs: self.regs,
            eip: self.eip,
            eflags: self.eflags,
            mem: Vec::new(),
            out_len: self.output.len(),
            input_taken: Vec::new(),
            frames: None,
            exited: false,
            prev_regs: self.prev_regs,
            prev_eflags: self.prev_eflags,
            prev_eip: self.prev_eip,
            last_writes: std::mem::take(&mut self.last_writes),
            counted: None,
            ticks: self.ticks,
            rng: self.rng,
            keys_taken: Vec::new(),
            brk: self.brk,
            external: false,
        };
    }

    fn rollback(&mut self, u: Undo) {
        self.regs = u.regs;
        self.eip = u.eip;
        self.eflags = u.eflags;
        for &(a, b) in u.mem.iter().rev() {
            self.mem[a as usize] = b;
        }
        self.output.truncate(u.out_len);
        for &b in u.input_taken.iter().rev() {
            self.input.push_front(b);
        }
        if let Some(f) = u.frames {
            self.frames = f;
        }
        self.prev_regs = u.prev_regs;
        self.prev_eflags = u.prev_eflags;
        self.prev_eip = u.prev_eip;
        self.last_writes = u.last_writes;
        if let Some(i) = u.counted {
            self.hits[i] -= 1;
        }
        self.ticks = u.ticks;
        self.rng = u.rng;
        self.brk = u.brk;
        for &k in u.keys_taken.iter().rev() {
            self.keys.push_front(k);
        }
    }

    /// xorshift64*: deterministic, and restored by step back.
    fn next_random(&mut self) -> u32 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }

    /// A key press from the Screen window (see the KEY_* codes in the docs).
    pub fn key_event(&mut self, code: u32, down: bool) {
        if let Some(d) = self.keys_down.get_mut(code as usize) {
            *d = down;
        }
        if down && self.keys.len() < 64 {
            self.keys.push_back(code);
        }
    }

    /// Fills a clipped rectangle of the screen with a colour (through `write`, so it's undoable).
    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32) -> R<()> {
        let (x0, y0) = (x.max(0), y.max(0));
        let (x1, y1) = (x.saturating_add(w).min(SCREEN_W as i32), y.saturating_add(h).min(SCREEN_H as i32));
        for py in y0..y1 {
            for px in x0..x1 {
                self.write(SCREEN_BASE + py as u32 * SCREEN_W + px as u32, Size::B, color & 0xFF)?;
            }
        }
        Ok(())
    }

    /// Executes one instruction.
    pub fn step(&mut self) -> StepEvent {
        if !matches!(self.status, Status::Ready | Status::WaitInput) {
            return StepEvent::Stopped;
        }
        self.status = Status::Ready;
        self.begin_undo();
        let before_regs = self.regs;
        let before_flags = self.eflags;
        let before_eip = self.eip;
        match self.exec() {
            Ok(Some(ev)) => {
                self.prev_regs = before_regs;
                self.prev_eflags = before_flags;
                self.prev_eip = before_eip;
                self.steps += 1;
                let u = std::mem::take(&mut self.cur);
                if u.external {
                    // A file was changed (or opened / read / moved): stepping back can't undo
                    // that, so the history starts again after this step.
                    self.history.clear();
                    self.history_bytes = 0;
                    return ev;
                }
                self.history_bytes += u.cost();
                self.history.push_back(u);
                while self.history.len() > HISTORY_LIMIT || self.history_bytes > HISTORY_BYTES {
                    let Some(old) = self.history.pop_front() else { break };
                    self.history_bytes -= old.cost();
                }
                ev
            }
            Ok(None) => {
                // Waiting for input: nothing happened.
                let u = std::mem::take(&mut self.cur);
                self.rollback(u);
                self.status = Status::WaitInput;
                StepEvent::Stopped
            }
            Err(msg) => {
                let u = std::mem::take(&mut self.cur);
                self.rollback(u);
                self.status = Status::Fault(msg);
                StepEvent::Stopped
            }
        }
    }

    /// Undoes the last step. Returns false if there is no history.
    pub fn step_back(&mut self) -> bool {
        if matches!(self.status, Status::Fault(_) | Status::WaitInput) {
            self.status = Status::Ready;
            return true;
        }
        let Some(u) = self.history.pop_back() else { return false };
        self.history_bytes -= u.cost();
        let exited = u.exited || matches!(self.status, Status::Exited(_));
        self.rollback(u);
        if exited {
            self.status = Status::Ready;
        }
        self.steps = self.steps.saturating_sub(1);
        self.last_op = self.prog.instr_at(self.prev_eip).map(|i| i.op);
        true
    }

    fn set_frames(&mut self, f: Vec<Frame>) {
        if self.cur.frames.is_none() {
            self.cur.frames = Some(self.frames.clone());
        }
        self.frames = f;
    }

    fn exec(&mut self) -> R<Option<StepEvent>> {
        if self.eip == EXIT_ADDR {
            // Returned from main: exit code is EAX, as in C.
            self.status = Status::Exited(self.regs[EAX as usize] as i32);
            self.cur.exited = true;
            self.last_op = Some(Op::Ret);
            return Ok(Some(StepEvent::Ok));
        }
        let prog = self.prog.clone();
        let Some(ins) = prog.instr_at(self.eip) else {
            if self.eip == prog.code_end() {
                return Err("Execution ran past the last instruction. End the program with an exit system call (mov eax, 1 / mov ebx, 0 / int 0x80), or 'ret' from main.".into());
            }
            return Err(format!("EIP = 0x{:08X} is not an instruction address (bad jump target or corrupted return address on the stack?)", self.eip));
        };
        let idx = ((self.eip - CODE_BASE) / INSTR_SIZE) as usize;
        self.hits[idx] += 1;
        self.cur.counted = Some(idx);
        self.last_op = Some(ins.op);
        let s = ins.size;
        let o = &ins.operands;
        let next = self.eip + INSTR_SIZE;
        let mut new_eip = next;
        let mut event = StepEvent::Ok;

        match ins.op {
            Op::Mov => {
                let v = self.get(&o[1], s)?;
                self.set(&o[0], s, v)?;
            }
            Op::Movzx | Op::Movsx => {
                let ss = match o[1] {
                    Operand::Reg(r) => r.size,
                    Operand::Mem(m) => m.size.unwrap_or(Size::B),
                    Operand::Imm(_) => Size::B,
                };
                let v = self.get(&o[1], ss)?;
                let v = if ins.op == Op::Movsx { ss.sext(v) } else { v };
                self.set(&o[0], s, v & s.mask())?;
            }
            Op::Lea => {
                let Operand::Mem(m) = o[1] else { unreachable!() };
                let a = self.ea(&m);
                self.set(&o[0], s, a & s.mask())?;
            }
            Op::Xchg => {
                let a = self.get(&o[0], s)?;
                let b = self.get(&o[1], s)?;
                self.set(&o[0], s, b)?;
                self.set(&o[1], s, a)?;
            }
            Op::Add | Op::Adc | Op::Sub | Op::Sbb | Op::Cmp => {
                let a = self.get(&o[0], s)?;
                let b = self.get(&o[1], s)?;
                let c = self.flag(CF) as u32;
                let r = match ins.op {
                    Op::Add => self.add(a, b, 0, s),
                    Op::Adc => self.add(a, b, c, s),
                    Op::Sbb => self.sub(a, b, c, s),
                    _ => self.sub(a, b, 0, s),
                };
                if ins.op != Op::Cmp {
                    self.set(&o[0], s, r)?;
                }
            }
            Op::And | Op::Or | Op::Xor | Op::Test => {
                let a = self.get(&o[0], s)?;
                let b = self.get(&o[1], s)?;
                let r = match ins.op {
                    Op::Or => a | b,
                    Op::Xor => a ^ b,
                    _ => a & b,
                };
                let r = self.logic(r, s);
                if ins.op != Op::Test {
                    self.set(&o[0], s, r)?;
                }
            }
            Op::Inc | Op::Dec => {
                let a = self.get(&o[0], s)?;
                let cf = self.flag(CF);
                let r = if ins.op == Op::Inc { self.add(a, 1, 0, s) } else { self.sub(a, 1, 0, s) };
                self.set_flag(CF, cf);
                self.set(&o[0], s, r)?;
            }
            Op::Neg => {
                let a = self.get(&o[0], s)?;
                let r = self.sub(0, a, 0, s);
                self.set_flag(CF, a != 0);
                self.set(&o[0], s, r)?;
            }
            Op::Not => {
                let a = self.get(&o[0], s)?;
                self.set(&o[0], s, !a & s.mask())?;
            }
            Op::Mul | Op::Imul if o.len() == 1 => {
                let signed = ins.op == Op::Imul;
                let src = self.get(&o[0], s)?;
                let acc = self.reg(Reg { idx: EAX, size: s });
                let (lo, hi, overflow) = if signed {
                    let p = s.sext(acc) as i32 as i64 * s.sext(src) as i32 as i64;
                    let lo = (p as u64 as u32) & s.mask();
                    let hi = ((p >> s.bits()) as u64 as u32) & s.mask();
                    (lo, hi, s.sext(lo) as i32 as i64 != p)
                } else {
                    let p = acc as u64 * src as u64;
                    let lo = (p as u32) & s.mask();
                    let hi = ((p >> s.bits()) as u32) & s.mask();
                    (lo, hi, hi != 0)
                };
                match s {
                    Size::B => self.set_reg(Reg { idx: EAX, size: Size::W }, (hi << 8) | lo),
                    Size::W | Size::D => {
                        self.set_reg(Reg { idx: EAX, size: s }, lo);
                        self.set_reg(Reg { idx: EDX, size: s }, hi);
                    }
                }
                self.set_flag(CF, overflow);
                self.set_flag(OF, overflow);
                self.szp(lo, s);
            }
            Op::Imul => {
                let a = self.get(&o[1], s)?;
                let b = if o.len() == 3 { o[2].imm_value() } else { self.get(&o[0], s)? };
                let p = s.sext(a) as i32 as i64 * s.sext(b & s.mask()) as i32 as i64;
                let r = (p as u64 as u32) & s.mask();
                let overflow = s.sext(r) as i32 as i64 != p;
                self.set_flag(CF, overflow);
                self.set_flag(OF, overflow);
                self.szp(r, s);
                self.set(&o[0], s, r)?;
            }
            Op::Mul => unreachable!(),
            Op::Div | Op::Idiv => {
                let signed = ins.op == Op::Idiv;
                let src = self.get(&o[0], s)?;
                if src == 0 {
                    return Err("Division by zero".into());
                }
                let (hi_reg, lo_reg, qname) = match s {
                    Size::B => ("AH", "AL", "AL"),
                    Size::W => ("DX", "AX", "AX"),
                    Size::D => ("EDX", "EAX", "EAX"),
                };
                let dividend: u64 = match s {
                    Size::B => self.reg(Reg { idx: EAX, size: Size::W }) as u64,
                    _ => ((self.reg(Reg { idx: EDX, size: s }) as u64) << s.bits()) | self.reg(Reg { idx: EAX, size: s }) as u64,
                };
                let hint = if signed {
                    format!("Before idiv, sign-extend {lo_reg} into {hi_reg} (cdq / cwd / cbw).")
                } else {
                    format!("Before div, clear {hi_reg} (e.g. xor edx, edx), since the dividend is {hi_reg}:{lo_reg}.")
                };
                let (q, r) = if signed {
                    let bits = s.bits() * 2;
                    let d = ((dividend << (64 - bits)) as i64) >> (64 - bits);
                    let v = s.sext(src) as i32 as i64;
                    let q = d / v;
                    let r = d % v;
                    let lim = 1i64 << (s.bits() - 1);
                    if q < -lim || q >= lim {
                        return Err(format!("Division overflow: the quotient doesn't fit in {qname}. {hint}"));
                    }
                    ((q as u64 as u32) & s.mask(), (r as u64 as u32) & s.mask())
                } else {
                    let q = dividend / src as u64;
                    let r = dividend % src as u64;
                    if q > s.mask() as u64 {
                        return Err(format!("Division overflow: the quotient doesn't fit in {qname}. {hint}"));
                    }
                    (q as u32, r as u32)
                };
                match s {
                    Size::B => self.set_reg(Reg { idx: EAX, size: Size::W }, (r << 8) | q),
                    _ => {
                        self.set_reg(Reg { idx: EAX, size: s }, q);
                        self.set_reg(Reg { idx: EDX, size: s }, r);
                    }
                }
            }
            Op::Cbw => {
                let v = Size::B.sext(self.regs[0]);
                self.set_reg(Reg { idx: EAX, size: Size::W }, v);
            }
            Op::Cwde => self.regs[0] = Size::W.sext(self.regs[0]),
            Op::Cwd => {
                let v = if self.regs[0] & 0x8000 != 0 { 0xFFFF } else { 0 };
                self.set_reg(Reg { idx: EDX, size: Size::W }, v);
            }
            Op::Cdq => self.regs[EDX as usize] = if (self.regs[0] as i32) < 0 { 0xFFFF_FFFF } else { 0 },
            Op::Shl | Op::Shr | Op::Sar | Op::Rol | Op::Ror => {
                let a = self.get(&o[0], s)?;
                let count = self.get(&o[1], Size::B)? & 31;
                if count != 0 {
                    let bits = s.bits();
                    let r = match ins.op {
                        Op::Shl => {
                            let r = if count >= 32 { 0 } else { (a << count) & s.mask() };
                            let cf = count <= bits && (a >> (bits - count)) & 1 != 0;
                            self.set_flag(CF, cf);
                            self.set_flag(OF, ((r & s.sign_bit()) != 0) != cf);
                            self.szp(r, s);
                            r
                        }
                        Op::Shr => {
                            let r = if count >= bits { 0 } else { a >> count };
                            self.set_flag(CF, (a >> (count - 1)) & 1 != 0);
                            self.set_flag(OF, a & s.sign_bit() != 0);
                            self.szp(r, s);
                            r
                        }
                        Op::Sar => {
                            let sa = s.sext(a) as i32;
                            let r = (sa >> count.min(31)) as u32 & s.mask();
                            self.set_flag(CF, (sa >> (count - 1).min(31)) & 1 != 0);
                            self.set_flag(OF, false);
                            self.szp(r, s);
                            r
                        }
                        Op::Rol => {
                            let c = count % bits;
                            let r = if c == 0 { a } else { ((a << c) | (a >> (bits - c))) & s.mask() };
                            self.set_flag(CF, r & 1 != 0);
                            self.set_flag(OF, ((r & s.sign_bit()) != 0) != (r & 1 != 0));
                            r
                        }
                        _ => {
                            let c = count % bits;
                            let r = if c == 0 { a } else { ((a >> c) | (a << (bits - c))) & s.mask() };
                            self.set_flag(CF, r & s.sign_bit() != 0);
                            self.set_flag(OF, ((r ^ (r << 1)) & s.sign_bit()) != 0);
                            r
                        }
                    };
                    self.set(&o[0], s, r)?;
                }
            }
            Op::Push => {
                let v = self.get(&o[0], s)?;
                self.push(s, v)?;
            }
            Op::Pop => {
                let v = self.pop(s)?;
                self.set(&o[0], s, v)?;
            }
            Op::Pushad => {
                let esp = self.regs[ESP as usize];
                for i in 0..8 {
                    let v = if i == ESP as usize { esp } else { self.regs[i] };
                    self.push(Size::D, v)?;
                }
            }
            Op::Popad => {
                for i in (0..8).rev() {
                    let v = self.pop(Size::D)?;
                    if i != ESP as usize {
                        self.regs[i] = v;
                    }
                }
            }
            Op::Pushfd => self.push(Size::D, self.eflags)?,
            Op::Popfd => {
                let v = self.pop(Size::D)?;
                let mask = CF | PF | ZF | SF | DF | OF;
                self.eflags = (v & mask) | FLAGS_FIXED;
            }
            Op::Call => {
                let target = match &o[0] {
                    Operand::Imm(v) => *v,
                    other => self.get(other, Size::D)?,
                };
                self.push(Size::D, next)?;
                let mut f = self.frames.clone();
                f.push(Frame { target, call_site: self.eip, ret_addr: next, esp: self.regs[ESP as usize] });
                if f.len() > 100_000 {
                    f.remove(0);
                }
                self.set_frames(f);
                new_eip = target;
            }
            Op::Ret => {
                new_eip = self.pop(Size::D)?;
                if let Some(Operand::Imm(n)) = o.first() {
                    self.regs[ESP as usize] = self.regs[ESP as usize].wrapping_add(*n);
                }
                let esp = self.regs[ESP as usize];
                if self.frames.last().is_some_and(|f| f.esp < esp) {
                    let mut f = self.frames.clone();
                    while f.last().is_some_and(|fr| fr.esp < esp) {
                        f.pop();
                    }
                    self.set_frames(f);
                }
            }
            Op::Jmp => {
                new_eip = match &o[0] {
                    Operand::Imm(v) => *v,
                    other => self.get(other, Size::D)?,
                };
            }
            Op::Jcc(c) => {
                if self.cond(c) {
                    new_eip = o[0].imm_value();
                }
            }
            Op::Loop => {
                let c = self.regs[ECX as usize].wrapping_sub(1);
                self.regs[ECX as usize] = c;
                if c != 0 {
                    new_eip = o[0].imm_value();
                }
            }
            Op::Jecxz => {
                if self.regs[ECX as usize] == 0 {
                    new_eip = o[0].imm_value();
                }
            }
            Op::Setcc(c) => {
                let v = self.cond(c) as u32;
                self.set(&o[0], Size::B, v)?;
            }
            Op::Cmovcc(c) => {
                let v = self.get(&o[1], s)?;
                if self.cond(c) {
                    self.set(&o[0], s, v)?;
                }
            }
            Op::Leave => {
                self.regs[ESP as usize] = self.regs[EBP as usize];
                self.regs[EBP as usize] = self.pop(Size::D)?;
            }
            Op::Nop => {}
            Op::Hlt => {
                self.status = Status::Exited(0);
                self.cur.exited = true;
            }
            Op::Int => match o[0].imm_value() {
                0x80 => {
                    if !self.syscall()? {
                        return Ok(None);
                    }
                    if std::mem::take(&mut self.frame_wait) {
                        event = StepEvent::Frame;
                    }
                }
                3 => event = StepEvent::Int3,
                n => return Err(format!("int 0x{n:X} is not supported; use int 0x80 for system calls")),
            },
            Op::Clc => self.set_flag(CF, false),
            Op::Stc => self.set_flag(CF, true),
            Op::Cmc => self.set_flag(CF, !self.flag(CF)),
            Op::Cld => self.set_flag(DF, false),
            Op::Std => self.set_flag(DF, true),
            Op::Movs | Op::Stos | Op::Lods | Op::Cmps | Op::Scas => self.string_op(ins.op, s, ins.rep)?,
            Op::Rcl | Op::Rcr => {
                // Rotate through carry: CF is an extra bit above the value.
                let a = self.get(&o[0], s)?;
                let bits = s.bits();
                let count = (self.get(&o[1], Size::B)? & 31) % (bits + 1);
                if count != 0 {
                    let mask = s.mask() as u64;
                    let wide = (a as u64) | ((self.flag(CF) as u64) << bits); // bits+1 bits
                    let n = bits as u64 + 1;
                    let c = count as u64;
                    let rotated = if ins.op == Op::Rcl { ((wide << c) | (wide >> (n - c))) & ((1 << n) - 1) } else { ((wide >> c) | (wide << (n - c))) & ((1 << n) - 1) };
                    let r = (rotated & mask) as u32;
                    self.set_flag(CF, (rotated >> bits) & 1 != 0);
                    let msb = r & s.sign_bit() != 0;
                    if ins.op == Op::Rcl {
                        self.set_flag(OF, msb != self.flag(CF));
                    } else {
                        self.set_flag(OF, msb != ((r << 1) & s.sign_bit() != 0));
                    }
                    self.set(&o[0], s, r)?;
                }
            }
            Op::Bt | Op::Bts | Op::Btr | Op::Btc => {
                // With memory and a register bit number, the number can be anything: a bit
                // array (bit 100 is in the dword at +12), as on a real x86.
                let raw = self.get(&o[1], s)?;
                let target = match (&o[0], &o[1]) {
                    (Operand::Mem(m), Operand::Reg(_)) => {
                        let signed = s.sext(raw) as i32 as i64;
                        let unit = s.bits() as i64;
                        let mut mm = *m;
                        mm.disp = (mm.disp as i64 + signed.div_euclid(unit) * (unit / 8)) as i32;
                        Operand::Mem(mm)
                    }
                    _ => o[0],
                };
                let o = [target, o[1]];
                let a = self.get(&o[0], s)?;
                let bit = raw % s.bits();
                self.set_flag(CF, (a >> bit) & 1 != 0);
                let r = match ins.op {
                    Op::Bts => a | (1 << bit),
                    Op::Btr => a & !(1 << bit),
                    Op::Btc => a ^ (1 << bit),
                    _ => a,
                };
                if ins.op != Op::Bt {
                    self.set(&o[0], s, r & s.mask())?;
                }
            }
            Op::Bsf | Op::Bsr => {
                let v = self.get(&o[1], s)?;
                self.set_flag(ZF, v == 0);
                if v != 0 {
                    let i = if ins.op == Op::Bsf { v.trailing_zeros() } else { 31 - v.leading_zeros() };
                    self.set(&o[0], s, i)?;
                }
            }
            Op::Popcnt => {
                let v = self.get(&o[1], s)?;
                for f in [CF, OF, SF, PF] {
                    self.set_flag(f, false);
                }
                self.set_flag(ZF, v == 0);
                self.set(&o[0], s, v.count_ones())?;
            }
            Op::Shld | Op::Shrd => {
                let d = self.get(&o[0], s)?;
                let src = self.get(&o[1], s)?;
                let count = self.get(&o[2], Size::B)? & 31;
                let bits = s.bits();
                if count != 0 && count <= bits {
                    let (d64, s64, c) = (d as u64, src as u64, count as u64);
                    let (r, cf) = if ins.op == Op::Shld {
                        let r = ((d64 << c) | (s64 >> (bits as u64 - c))) as u32 & s.mask();
                        (r, (d >> (bits - count)) & 1 != 0)
                    } else {
                        let r = ((d64 >> c) | (s64 << (bits as u64 - c))) as u32 & s.mask();
                        (r, (d >> (count - 1)) & 1 != 0)
                    };
                    self.set_flag(CF, cf);
                    self.set_flag(OF, (r & s.sign_bit()) != (d & s.sign_bit()));
                    self.szp(r, s);
                    self.set(&o[0], s, r)?;
                }
            }
            Op::Bswap => {
                let v = self.get(&o[0], Size::D)?;
                self.set(&o[0], Size::D, v.swap_bytes())?;
            }
            Op::Xadd => {
                let a = self.get(&o[0], s)?;
                let b = self.get(&o[1], s)?;
                let r = self.add(a, b, 0, s);
                self.set(&o[1], s, a)?;
                self.set(&o[0], s, r)?;
            }
            Op::Cmpxchg => {
                let acc = Reg { idx: EAX, size: s };
                let a = self.reg(acc);
                let d = self.get(&o[0], s)?;
                self.sub(a, d, 0, s);
                if a == d {
                    let v = self.get(&o[1], s)?;
                    self.set(&o[0], s, v)?;
                } else {
                    self.set_reg(acc, d);
                }
            }
            Op::Lahf => {
                // AH = SF ZF 0 AF 0 PF 1 CF (AF is not modelled: 0).
                let v = (self.eflags & (SF | ZF | PF | CF)) | 0x02;
                self.set_reg(Reg { idx: 4, size: Size::B }, v & 0xFF);
            }
            Op::Sahf => {
                let ah = self.reg(Reg { idx: 4, size: Size::B });
                for f in [SF, ZF, PF, CF] {
                    self.set_flag(f, ah & f != 0);
                }
            }
            Op::Loope | Op::Loopne => {
                let c = self.regs[ECX as usize].wrapping_sub(1);
                self.regs[ECX as usize] = c;
                let zf = self.flag(ZF);
                if c != 0 && (if ins.op == Op::Loope { zf } else { !zf }) {
                    new_eip = o[0].imm_value();
                }
            }
            Op::Xlat => {
                let al = self.regs[EAX as usize] & 0xFF;
                let v = self.read(self.regs[EBX as usize].wrapping_add(al), Size::B)?;
                self.set_reg(Reg { idx: EAX, size: Size::B }, v);
            }
            Op::Enter => {
                // enter size, 0 = push ebp / mov ebp, esp / sub esp, size
                self.push(Size::D, self.regs[EBP as usize])?;
                self.regs[EBP as usize] = self.regs[ESP as usize];
                self.regs[ESP as usize] = self.regs[ESP as usize].wrapping_sub(o[0].imm_value());
            }
        }
        self.eip = new_eip;
        Ok(Some(event))
    }

    fn string_op(&mut self, op: Op, s: Size, rep: Rep) -> R<()> {
        let delta = if self.flag(DF) { (s.bytes() as u32).wrapping_neg() } else { s.bytes() };
        let acc = Reg { idx: EAX, size: s };
        let mut iterations = 0u32;
        loop {
            if rep != Rep::None {
                if self.regs[ECX as usize] == 0 {
                    break;
                }
                iterations += 1;
                if iterations > 1_000_000 {
                    return Err("rep: more than 1,000,000 iterations (is ECX set correctly?)".into());
                }
            }
            let (esi, edi) = (self.regs[ESI as usize], self.regs[EDI as usize]);
            match op {
                Op::Movs => {
                    let v = self.read(esi, s)?;
                    self.write(edi, s, v)?;
                    self.regs[ESI as usize] = esi.wrapping_add(delta);
                    self.regs[EDI as usize] = edi.wrapping_add(delta);
                }
                Op::Stos => {
                    let v = self.reg(acc);
                    self.write(edi, s, v)?;
                    self.regs[EDI as usize] = edi.wrapping_add(delta);
                }
                Op::Lods => {
                    let v = self.read(esi, s)?;
                    self.set_reg(acc, v);
                    self.regs[ESI as usize] = esi.wrapping_add(delta);
                }
                Op::Cmps => {
                    let a = self.read(esi, s)?;
                    let b = self.read(edi, s)?;
                    self.sub(a, b, 0, s);
                    self.regs[ESI as usize] = esi.wrapping_add(delta);
                    self.regs[EDI as usize] = edi.wrapping_add(delta);
                }
                _ => {
                    let a = self.reg(acc);
                    let b = self.read(edi, s)?;
                    self.sub(a, b, 0, s);
                    self.regs[EDI as usize] = edi.wrapping_add(delta);
                }
            }
            if rep == Rep::None {
                break;
            }
            self.regs[ECX as usize] = self.regs[ECX as usize].wrapping_sub(1);
            if (rep == Rep::Repe && !self.flag(ZF)) || (rep == Rep::Repne && self.flag(ZF)) {
                break;
            }
        }
        Ok(())
    }
}

trait ImmValue {
    fn imm_value(&self) -> u32;
}

impl ImmValue for Operand {
    fn imm_value(&self) -> u32 {
        match self {
            Operand::Imm(v) => *v,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str, input: &str) -> Machine {
        let (p, d) = assemble(src);
        assert!(d.iter().all(|d| d.severity != Severity::Error), "{d:#?}");
        let mut m = Machine::new(Rc::new(p));
        m.input.extend(input.bytes());
        for _ in 0..1_000_000 {
            if m.step() == StepEvent::Stopped {
                break;
            }
        }
        m
    }

    #[test]
    fn hello_world() {
        let m = run("section .data\nmsg db 'Hello', 10\nlen equ $-msg\nsection .text\n_start:\nmov eax,4\nmov ebx,1\nmov ecx,msg\nmov edx,len\nint 0x80\nmov eax,1\nmov ebx,7\nint 0x80\n", "");
        assert_eq!(m.output, "Hello\n");
        assert_eq!(m.status, Status::Exited(7));
    }

    #[test]
    fn recursion_factorial() {
        let src = "main:\n push 5\n call fact\n add esp, 4\n mov ebx, eax\n mov eax, 200\n int 0x80\n mov eax, 0\n ret\nfact:\n push ebp\n mov ebp, esp\n mov eax, [ebp+8]\n cmp eax, 1\n jle .done\n dec eax\n push eax\n call fact\n add esp, 4\n imul eax, [ebp+8]\n.done:\n pop ebp\n ret\n";
        let m = run(src, "");
        assert_eq!(m.output, "120");
        assert_eq!(m.status, Status::Exited(0));
        assert!(m.frames.is_empty());
    }

    #[test]
    fn flags() {
        let m = run("mov al, 0x7F\nadd al, 1\nhlt", "");
        assert!(m.flag(OF) && m.flag(SF) && !m.flag(CF) && !m.flag(ZF));
        let m = run("mov eax, 0\nsub eax, 1\nhlt", "");
        assert!(m.flag(CF) && m.flag(SF) && !m.flag(OF));
        let m = run("mov eax, 5\ncmp eax, 5\nsete bl\nhlt", "");
        assert_eq!(m.regs[EBX as usize] & 0xFF, 1);
    }

    #[test]
    fn div_and_faults() {
        let m = run("mov eax, 100\nxor edx, edx\nmov ecx, 7\ndiv ecx\nhlt", "");
        assert_eq!((m.regs[0], m.regs[2]), (14, 2));
        let m = run("mov eax, -100\ncdq\nmov ecx, 7\nidiv ecx\nhlt", "");
        assert_eq!((m.regs[0] as i32, m.regs[2] as i32), (-14, -2));
        let m = run("mov eax, 1\nmov edx, 5\nmov ecx, 1\ndiv ecx\nhlt", "");
        assert!(matches!(m.status, Status::Fault(ref s) if s.contains("overflow")));
        assert_eq!(m.eip, CODE_BASE + 3 * INSTR_SIZE);
        let m = run("mov eax, [0]", "");
        assert!(matches!(m.status, Status::Fault(_)));
        let m = run("f: call f", "");
        assert!(matches!(m.status, Status::Fault(ref s) if s.contains("Stack overflow")));
    }

    #[test]
    fn step_back_restores_everything() {
        let (p, _) = assemble("section .bss\nx resd 1\nsection .text\nmov eax, 4\nmov ebx, 1\nmov ecx, msg\nmov edx, 2\nint 0x80\nmov dword [x], 42\nhlt\nsection .data\nmsg db 'ok'");
        let mut m = Machine::new(Rc::new(p));
        for _ in 0..7 {
            m.step();
        }
        assert_eq!(m.output, "ok");
        assert_eq!(m.status, Status::Exited(0));
        while m.step_back() {}
        assert_eq!(m.output, "");
        assert_eq!(m.regs[0], 0);
        assert_eq!(m.peek(m.prog.bss_base, 4), Some(0));
        assert_eq!(m.eip, CODE_BASE);
        assert_eq!(m.status, Status::Ready);
    }

    #[test]
    fn input_and_strings() {
        let src = "section .bss\nbuf resb 16\nsection .text\nmov eax, 203\nint 0x80\nmov esi, eax\nmov eax, 3\nmov ebx, 0\nmov ecx, buf\nmov edx, 16\nint 0x80\nmov edi, buf\nmov al, 'x'\nmov ecx, 16\ncld\nrepne scasb\nhlt";
        let m = run(src, "42\nabxd\n");
        assert_eq!(m.regs[ESI as usize], 42);
        assert_eq!(m.regs[EDI as usize], m.prog.bss_base + 3);
    }

    #[test]
    fn waits_for_input() {
        let (p, _) = assemble("mov eax, 203\nint 0x80\nhlt");
        let mut m = Machine::new(Rc::new(p));
        m.step();
        assert_eq!(m.step(), StepEvent::Stopped);
        assert_eq!(m.status, Status::WaitInput);
        m.input.extend(b"-5\n");
        m.step();
        assert_eq!(m.regs[0] as i32, -5);
    }

    /// Runs `body` (instructions) and returns the machine; the program ends with hlt (so the
    /// registers are left as the body set them).
    fn exec(body: &str) -> Machine {
        let m = run(&format!("section .data\nbuf times 64 db 0\nsection .text\n_start:\n{body}\nhlt\n"), "");
        assert_eq!(m.status, Status::Exited(0), "{body}");
        m
    }

    #[test]
    fn bit_instructions() {
        let m = exec("mov eax, 0b1010\nbt eax, 1\nsetc bl\nbt eax, 2\nsetc bh\nbts eax, 0\nbtr eax, 3\nbtc eax, 31");
        assert_eq!(m.regs[EBX as usize] & 0xFFFF, 0x0001, "bit 1 set, bit 2 clear");
        assert_eq!(m.regs[EAX as usize], 0x8000_0003);
        let m = exec("mov eax, 0b101000\nbsf ecx, eax\nbsr edx, eax\npopcnt esi, eax\nmov edi, 77\nxor ebx, ebx\nbsf edi, ebx");
        assert_eq!((m.regs[ECX as usize], m.regs[EDX as usize], m.regs[ESI as usize]), (3, 5, 2));
        assert_eq!(m.regs[EDI as usize], 77, "bsf of 0 leaves the destination");
        assert!(m.flag(ZF));
        // A bit array in memory: bit 100 is bit 4 of the byte at +12.
        let m = exec("mov ecx, 100\nbts [buf], ecx\nbt [buf], ecx\nsetc al");
        assert_eq!(m.peek(DATA_BASE + 12, 1), Some(0x10));
        assert_eq!(m.regs[EAX as usize] & 0xFF, 1);
        let m = exec("mov eax, 0x11223344\nbswap eax");
        assert_eq!(m.regs[EAX as usize], 0x4433_2211);
    }

    #[test]
    fn rotates_through_carry_and_double_shifts() {
        // 64-bit shift left by 1 of edx:eax with shl + rcl.
        let m = exec("mov eax, 0x80000001\nmov edx, 1\nshl eax, 1\nrcl edx, 1");
        assert_eq!((m.regs[EDX as usize], m.regs[EAX as usize]), (3, 2));
        let m = exec("mov edx, 3\nmov eax, 2\nshr edx, 1\nrcr eax, 1");
        assert_eq!((m.regs[EDX as usize], m.regs[EAX as usize]), (1, 0x8000_0001));
        let m = exec("mov edx, 0x12345678\nmov eax, 0x9ABCDEF0\nshld edx, eax, 8");
        assert_eq!(m.regs[EDX as usize], 0x3456_789A);
        let m = exec("mov eax, 0x12345678\nmov edx, 0x9ABCDEF0\nshrd eax, edx, 8");
        assert_eq!(m.regs[EAX as usize], 0xF012_3456);
    }

    #[test]
    fn exchange_flags_loops_xlat_enter() {
        let m = exec("mov dword [buf], 10\nmov eax, 5\nxadd [buf], eax");
        assert_eq!((m.peek(DATA_BASE, 4), m.regs[EAX as usize]), (Some(15), 10));
        let m = exec("mov dword [buf], 7\nmov eax, 7\nmov ecx, 99\ncmpxchg [buf], ecx\nsetz bl\nmov eax, 1\ncmpxchg [buf], ecx\nsetz bh");
        assert_eq!(m.peek(DATA_BASE, 4), Some(99));
        assert_eq!(m.regs[EBX as usize] & 0xFFFF, 0x0001, "first swapped, second not");
        assert_eq!(m.regs[EAX as usize], 99, "the failed one loads the current value");
        let m = exec("stc\nlahf\nmov bl, ah\nclc\nsahf\nsetc bh");
        assert_eq!(m.regs[EBX as usize] & 0xFF00, 0x0100, "sahf restored CF");
        assert_eq!(m.regs[EBX as usize] & 0x01, 1);
        // loopne: stop at the first 3 (index 2 of 1, 2, 3, 4).
        let m = exec("mov byte [buf], 1\nmov byte [buf+1], 2\nmov byte [buf+2], 3\nmov ecx, 4\nmov esi, buf\nnext:\nlodsb\ncmp al, 3\nloopne next");
        assert_eq!(m.regs[ESI as usize], DATA_BASE + 3);
        let m = exec("mov byte [buf+5], 42\nmov ebx, buf\nmov al, 5\nxlatb");
        assert_eq!(m.regs[EAX as usize] & 0xFF, 42);
        let m = exec("mov ebp, 123\nmov esi, esp\nenter 8, 0\nmov edi, esp\nleave");
        assert_eq!(m.regs[ESI as usize] - m.regs[EDI as usize], 12, "ebp pushed + 8 bytes of locals");
        assert_eq!(m.regs[EBP as usize], 123);
    }

    #[test]
    fn new_instructions_check_their_operands() {
        for bad in ["bswap ax", "bsf al, bl", "enter 8, 1", "shld eax, 5, 1", "xadd eax, 5", "bt al, 1"] {
            let (_, d) = assemble(&format!("_start:\n{bad}\n"));
            assert!(d.iter().any(|d| d.severity == Severity::Error), "{bad} should not assemble");
        }
    }

    #[test]
    fn file_system_calls() {
        let root = std::env::temp_dir().join(format!("atb_cpu_files_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let src = "section .data\n\
            name db 'out.txt', 0\n\
            text db 'Hi file', 10\n\
            len equ $ - text\n\
            section .bss\n\
            buf resb 32\n\
            section .text\n\
            _start:\n\
            mov eax, 8\nmov ebx, name\nint 0x80\nmov esi, eax\n\
            mov eax, 4\nmov ebx, esi\nmov ecx, text\nmov edx, len\nint 0x80\n\
            mov eax, 6\nmov ebx, esi\nint 0x80\n\
            mov eax, 5\nmov ebx, name\nmov ecx, O_RDONLY\nint 0x80\nmov esi, eax\n\
            mov eax, 19\nmov ebx, esi\nmov ecx, 0\nmov edx, SEEK_END\nint 0x80\nmov edi, eax\n\
            mov eax, 19\nmov ebx, esi\nmov ecx, 3\nmov edx, SEEK_SET\nint 0x80\n\
            mov eax, 3\nmov ebx, esi\nmov ecx, buf\nmov edx, 32\nint 0x80\n\
            mov eax, 4\nmov ebx, 1\nmov ecx, buf\nmov edx, 5\nint 0x80\n\
            mov eax, 6\nmov ebx, esi\nint 0x80\n\
            mov eax, 5\nmov ebx, nothing\nmov ecx, 0\nint 0x80\nmov ebp, eax\n\
            mov eax, 1\nmov ebx, 0\nint 0x80\n\
            section .data\nnothing db 'missing.txt', 0\n";
        let (p, d) = assemble(src);
        assert!(d.is_empty(), "{d:#?}");
        let mut m = Machine::new(Rc::new(p));
        m.files = crate::files::Sandbox::new(Some(root.clone()));
        for _ in 0..1000 {
            if m.step() == StepEvent::Stopped {
                break;
            }
        }
        assert_eq!(m.status, Status::Exited(0));
        assert_eq!(std::fs::read_to_string(root.join("out.txt")).unwrap(), "Hi file\n");
        assert_eq!(m.regs[EDI as usize], 8, "lseek to the end = the size");
        assert_eq!(m.output, "file\n", "read from position 3");
        assert_eq!(m.regs[EBP as usize] as i32, -2, "open of a missing file: -ENOENT");
        // Stepping back works for the 4 steps after the last file operation (open of the
        // missing file), but not past it: files can't be "un-written".
        let mut back = 0;
        while m.step_back() {
            back += 1;
        }
        assert_eq!(back, 4, "no stepping back past a file operation");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn brk_heap_time_getpid() {
        let m = exec("mov eax, 45\nmov ebx, 0\nint 0x80\nmov esi, eax\nlea ebx, [eax + 100]\nmov eax, 45\nint 0x80\nmov edi, eax\nmov eax, 45\nmov ebx, 0x100000\nint 0x80\nmov ebp, eax");
        let start = m.regs[ESI as usize];
        assert!(start >= DATA_BASE + 64 && start % 16 == 0, "after .data, aligned: {start:#X}");
        assert_eq!(m.regs[EDI as usize], start + 100);
        assert_eq!(m.regs[EBP as usize], start + 100, "beyond the screen memory: refused");
        let m = exec("mov eax, 13\nmov ebx, buf\nint 0x80\nmov esi, eax\nmov eax, 20\nint 0x80");
        assert!(m.regs[ESI as usize] > 1_700_000_000, "seconds since 1970");
        assert_eq!(m.peek(DATA_BASE, 4), Some(m.regs[ESI as usize]));
        assert_eq!(m.regs[EAX as usize], 1);
    }
}
