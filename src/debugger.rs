//! Run control on top of [`Machine`]: continue, step over / out, run to cursor, breakpoints.

use std::collections::HashSet;
use std::time::Instant;

use crate::asm::{Op, ESP, INSTR_SIZE};
use crate::cpu::{Machine, Status, StepEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Paused.
    Idle,
    Run,
    StepOver { ret: u32, esp: u32 },
    StepOut { esp: u32 },
    RunTo(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    Breakpoint,
    Int3,
    /// Step over / out / run-to reached its target.
    Done,
    Exited,
    Fault,
    WaitInput,
}

pub struct Debugger {
    pub m: Machine,
    pub mode: Mode,
    /// Skip the breakpoint check on the first instruction after resuming.
    resumed: bool,
}

impl Debugger {
    pub fn new(m: Machine) -> Self {
        Debugger { m, mode: Mode::Idle, resumed: true }
    }

    pub fn running(&self) -> bool {
        self.mode != Mode::Idle
    }

    /// Resumes from where execution is paused: a breakpoint on the current instruction is not
    /// hit again (it already stopped us, or the user stepped onto it).
    pub fn start(&mut self, mode: Mode) {
        self.mode = mode;
        self.resumed = true;
    }

    /// Starts a session that hasn't paused anywhere yet (Run from scratch): a breakpoint on
    /// the very first instruction stops it.
    pub fn start_fresh(&mut self, mode: Mode) {
        self.mode = mode;
        self.resumed = false;
    }

    /// Step over: like step into, but runs a `call` through to its return.
    pub fn step_over_mode(&self) -> Option<Mode> {
        let ins = self.m.current_instr()?;
        (ins.op == Op::Call).then(|| Mode::StepOver { ret: self.m.eip + INSTR_SIZE, esp: self.m.regs[ESP as usize] })
    }

    pub fn step_out_mode(&self) -> Mode {
        Mode::StepOut { esp: self.m.regs[ESP as usize] }
    }

    /// Runs up to `max_steps` instructions (or until `deadline`) in the current mode.
    pub fn run_slice(&mut self, breakpoints: &HashSet<u32>, max_steps: u64, deadline: Instant) -> Option<Stop> {
        let mut n = 0u64;
        while self.mode != Mode::Idle {
            if !self.resumed && breakpoints.contains(&self.m.eip) {
                return self.stop(Stop::Breakpoint);
            }
            self.resumed = false;
            let ev = self.m.step();
            match ev {
                StepEvent::Stopped => {
                    return match self.m.status {
                        Status::WaitInput => {
                            // Keep the mode: execution continues once input arrives.
                            self.resumed = true;
                            Some(Stop::WaitInput)
                        }
                        Status::Exited(_) => self.stop(Stop::Exited),
                        _ => self.stop(Stop::Fault),
                    };
                }
                StepEvent::Int3 => return self.stop(Stop::Int3),
                StepEvent::Ok | StepEvent::Frame => {}
            }
            if let Status::Exited(_) = self.m.status {
                return self.stop(Stop::Exited);
            }
            let esp = self.m.regs[ESP as usize];
            let done = match self.mode {
                Mode::StepOver { ret, esp: e0 } => self.m.eip == ret && esp >= e0,
                Mode::StepOut { esp: e0 } => self.m.last_op == Some(Op::Ret) && esp > e0,
                Mode::RunTo(a) => self.m.eip == a,
                _ => false,
            };
            if done {
                return self.stop(Stop::Done);
            }
            n += 1;
            // wait_frame ends the slice, so animations run at the display rate.
            if ev == StepEvent::Frame || n >= max_steps || (n % 1024 == 0 && Instant::now() >= deadline) {
                return None;
            }
        }
        None
    }

    fn stop(&mut self, s: Stop) -> Option<Stop> {
        self.mode = Mode::Idle;
        self.resumed = true;
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asm::{assemble, CODE_BASE, INSTR_SIZE};
    use std::rc::Rc;
    use std::time::Duration;

    fn dbg(src: &str) -> Debugger {
        let (p, _) = assemble(src);
        Debugger::new(Machine::new(Rc::new(p)))
    }

    fn run(d: &mut Debugger, bps: &HashSet<u32>) -> Option<Stop> {
        d.run_slice(bps, u64::MAX, Instant::now() + Duration::from_secs(5))
    }

    const SRC: &str = "_start:\nmov eax, 1\nmov ebx, 2\nmov ecx, 3\nhlt\n";

    #[test]
    fn fresh_run_stops_at_breakpoint_on_first_instruction() {
        let mut d = dbg(SRC);
        let bps: HashSet<u32> = [CODE_BASE].into();
        d.start_fresh(Mode::Run);
        assert_eq!(run(&mut d, &bps), Some(Stop::Breakpoint));
        assert_eq!(d.m.eip, CODE_BASE);
        assert_eq!(d.m.steps, 0);
        // Continuing from the breakpoint doesn't hit it again.
        d.start(Mode::Run);
        assert_eq!(run(&mut d, &bps), Some(Stop::Exited));
    }

    #[test]
    fn resume_skips_only_the_current_instruction() {
        let mut d = dbg(SRC);
        let second = CODE_BASE + INSTR_SIZE;
        let bps: HashSet<u32> = [CODE_BASE, second].into();
        // Paused at the entry (as after F10/F11): Continue skips it but stops at the next one.
        d.start(Mode::Run);
        assert_eq!(run(&mut d, &bps), Some(Stop::Breakpoint));
        assert_eq!(d.m.eip, second);
    }

    #[test]
    fn run_to_cursor_from_scratch() {
        let mut d = dbg(SRC);
        let third = CODE_BASE + 2 * INSTR_SIZE;
        d.start_fresh(Mode::RunTo(third));
        assert_eq!(run(&mut d, &HashSet::new()), Some(Stop::Done));
        assert_eq!(d.m.eip, third);
    }
}
