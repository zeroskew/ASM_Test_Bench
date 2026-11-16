//! Example programs, embedded in the exe at build time (build.rs picks up every
//! `examples/*.asm`, so adding an example only needs the file).

include!(concat!(env!("OUT_DIR"), "/examples.rs"));

pub struct Example {
    /// Title: the first comment line after the `; ====` banner, or the file name.
    pub title: String,
    /// File name, e.g. "03_factorial.asm".
    pub file_name: &'static str,
    pub source: &'static str,
}

fn title_of(file_name: &str, source: &str) -> String {
    source
        .lines()
        .take(6)
        .map(|l| l.trim_start_matches(';').trim())
        .find(|l| !l.is_empty() && !l.starts_with('='))
        .map(str::to_string)
        .unwrap_or_else(|| file_name.trim_end_matches(".asm").to_string())
}

/// All embedded examples, sorted by file name.
pub fn list() -> Vec<Example> {
    EMBEDDED.iter().map(|&(file_name, source)| Example { title: title_of(file_name, source), file_name, source }).collect()
}

/// The tutorial lessons (tutorial/*.asm), in order.
pub fn tutorial() -> Vec<Example> {
    TUTORIAL.iter().map(|&(file_name, source)| Example { title: title_of(file_name, source), file_name, source }).collect()
}

/// The advanced tutorial (advanced/*.asm), in order.
pub fn advanced() -> Vec<Example> {
    ADVANCED.iter().map(|&(file_name, source)| Example { title: title_of(file_name, source), file_name, source }).collect()
}

/// Source of an embedded example or (advanced) tutorial lesson by file name.
pub fn source(file_name: &str) -> Option<&'static str> {
    EMBEDDED.iter().chain(TUTORIAL).chain(ADVANCED).find(|(n, _)| *n == file_name).map(|(_, s)| *s)
}

#[cfg(test)]
mod tests {
    use crate::asm::{assemble, Severity};
    use crate::cpu::{Machine, Status};
    use std::rc::Rc;

    #[test]
    fn all_examples_are_embedded() {
        let on_disk = std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples")).unwrap().flatten().filter(|e| e.path().extension().is_some_and(|x| x == "asm")).count();
        assert_eq!(super::list().len(), on_disk);
        assert_eq!(super::list()[0].title, "Hello, World!");
    }

    #[test]
    fn text_examples_run() {
        // (file, console input, expected output)
        const DEFAULT_INPUT: &[u8] = b"Ada\n12\n";
        let expected: [(&str, &[u8], &str); 11] = [
            ("01_hello.asm", DEFAULT_INPUT, "Hello, World!\n"),
            ("02_sum_loop.asm", DEFAULT_INPUT, "Sum of 1..10 = 55\n"),
            ("03_factorial.asm", DEFAULT_INPUT, "720\n"),
            ("04_fibonacci.asm", DEFAULT_INPUT, "0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55, 89, 144, 233, 377\n"),
            ("05_array_max.asm", DEFAULT_INPUT, "Maximum: 42\n"),
            ("06_bubble_sort.asm", DEFAULT_INPUT, "1 2 3 4 5 6 7 8 9 \n"),
            ("07_strings.asm", DEFAULT_INPUT, "ASSEMBLY IS FUN!\n!NUF SI YLBMESSA\n"),
            ("08_input.asm", DEFAULT_INPUT, "What is your name? Nice to meet you, Ada\nEnter a number: Its square is 144\n"),
            ("09_flags.asm", DEFAULT_INPUT, ""),
            ("10_binary.asm", DEFAULT_INPUT, "11111101010\n"),
            ("14_add_two_numbers.asm", b"5\n37\n", "First number: Second number: Sum: 42\n"),
        ];
        for (file, input, want) in expected {
            let src = super::source(file).unwrap_or_else(|| panic!("{file} not embedded"));
            let (p, d) = assemble(src);
            let errs: Vec<_> = d.iter().filter(|d| d.severity == Severity::Error).collect();
            assert!(errs.is_empty(), "{file}: {errs:#?}");
            assert!(d.is_empty(), "{file}: warnings {d:#?}");
            let mut m = Machine::new(Rc::new(p));
            m.input.extend(input);
            for _ in 0..100_000 {
                m.step();
                if m.status != Status::Ready {
                    break;
                }
            }
            assert_eq!(m.status, Status::Exited(0), "{file}: {:?}", m.output);
            assert_eq!(m.output, want, "{file}");
        }
    }
}

#[cfg(test)]
mod tutorial_tests {
    use crate::asm::{assemble, SCREEN_BASE, SCREEN_W};
    use crate::cpu::{Machine, Status};
    use std::rc::Rc;

    /// Every lesson assembles without errors or warnings, exits with 0 and prints exactly what
    /// its comments promise.
    #[test]
    fn lessons_run_and_print_what_they_say() {
        let expected: &[(&str, &str)] = &[
            ("lesson_01_first_program.asm", ""),
            ("lesson_02_printing_text.asm", "Hello, assembly!\n"),
            ("lesson_03_registers.asm", "esi = 1234\neax in hex = 0x12345678\nafter mov al, 0xFF: eax = 0x123456FF\n"),
            ("lesson_04_arithmetic.asm", "10\n42\n40\n41\n-41\n-82\n"),
            ("lesson_05_multiply_divide.asm", "7 * 6 = 42\n47 / 5 = 9 remainder 2\n-47 / 5 = -9 remainder -2\n"),
            ("lesson_06_compare_and_jump.asm", "25 is bigger\n-5 is negative\n"),
            ("lesson_07_loops.asm", "1 2 3 4 5 \nsum of 1..10 = 55\ncountdown: 5 4 3 2 1 \n"),
            ("lesson_08_memory.asm", "count = 115\nsmall = 5\ncopy  = 115\nthe address of count = 0x00004000\n"),
            ("lesson_09_arrays.asm", "scores: 72 95 61 88 79 100 54 91 \nsum: 640\naverage: 80\nbest: 100\n"),
            ("lesson_10_stack.asm", "reversed: 5 4 3 2 1 \nafter the swap: eax = 222, ebx = 111\n"),
            ("lesson_11_functions.asm", "square(9) = 81\nmax(-3, 7) = 7\npower(2, 10) = 1024\n"),
            ("lesson_12_stack_frames.asm", "area(6, 7) = 42\nsum_to(100) = 5050\n"),
            ("lesson_13_strings.asm", "length: 16\nupper case: ASSEMBLY IS FUN!\nvowels: 4\n"),
            ("lesson_14_input.asm", "First number: Second number: Their sum is 42\nThe bigger one is 30\nYour name: Hello, Ada!\n"),
            ("lesson_15_bits.asm", "0b1100 and 0b1010 = 8\n0b1100 or  0b1010 = 14\n0b1100 xor 0b1010 = 6\n5 shl 3 = 40\n100 shr 2 = 25\nbits set in 0b10110111: 6\n13 is odd\n"),
            ("lesson_16_screen.asm", ""),
            ("lesson_17_files.asm", "File size: 50\nThe file says:\nAssembly can write files!\nThis line was appended.\n"),
        ];
        assert_eq!(super::tutorial().len(), expected.len(), "a lesson without an expected output here");
        for (file, want) in expected {
            let src = super::source(file).unwrap_or_else(|| panic!("{file} not embedded"));
            let (p, d) = assemble(src);
            assert!(d.is_empty(), "{file}: {d:#?}");
            let mut m = Machine::new(Rc::new(p));
            m.input.extend(b"12\n30\nAda\n");
            // Lesson 17 uses files: a temporary folder.
            m.files = crate::files::Sandbox::new(Some(std::env::temp_dir().join(format!("atb_lessons_{}", std::process::id()))));
            for _ in 0..1_000_000 {
                m.step();
                if m.status != Status::Ready {
                    break;
                }
            }
            assert_eq!(m.status, Status::Exited(0), "{file}: {:?}", m.output);
            assert_eq!(m.output, *want, "{file}");
            if file.contains("screen") {
                let px = |x: u32, y: u32| m.mem[(SCREEN_BASE + y * SCREEN_W + x) as usize];
                assert_eq!(px(64, 10), 15); // white pixel
                assert_eq!(px(10, 20), 11); // yellow line
                assert_eq!(px(45, 55), 9); // red square
                assert_eq!(px(70, 40), 232); // diagonal, first grey
            }
        }
        // Titles for the menu: "Lesson N: ..."
        assert!(super::tutorial().iter().all(|l| l.title.starts_with("Lesson ")));
    }
}

#[cfg(test)]
mod advanced_tests {
    use crate::asm::assemble;
    use crate::cpu::{Machine, Status};
    use std::rc::Rc;

    /// Every advanced lesson assembles cleanly, exits with 0 and prints what it promises.
    #[test]
    fn advanced_lessons_run_and_print_what_they_say() {
        let expected: &[(&str, &str)] = &[
            (
                "adv_01_recursion.asm",
                "Move disk 1 from A to C\nMove disk 2 from A to B\nMove disk 1 from C to B\nMove disk 3 from A to C\n\
                 Move disk 1 from B to A\nMove disk 2 from B to C\nMove disk 1 from A to C\nMoves: 7\n",
            ),
            (
                "adv_02_64bit_math.asm",
                "0x1FFFFFFFF + 1     = 0x0000000200000000\n100000 * 300000     = 0x00000006FC23AC00\n\
                 that << 4           = 0x0000006FC23AC000\nthat - 1            = 0x0000006FC23ABFFF\n",
            ),
            (
                "adv_03_bit_arrays.asm",
                "Primes below 100:\n2 3 5 7 11 13 17 19 23 29 31 37 41 43 47 53 59 61 67 71 73 79 83 89 97 \nCount: 25\nfloor(log2(1000)) = 9\n",
            ),
            ("adv_04_string_instructions.asm", "strlen(s1) = 8\ncopy: assembly\nstrcmp(s1, s2) = 20 (first difference at index 7)\n====================\n"),
            ("adv_05_heap_linked_list.asm", "list:     25 16 9 4 1 \nreversed: 1 4 9 16 25 \nheap used: 40 bytes\n"),
            ("adv_06_file_processing.asm", "lines:  4\nwords:  14\nchars:  70\nchunks: 5\ncleaned up\n"),
            ("adv_07_number_conversion.asm", "atoi(\"-12345\") = -12345\ntimes 2 = -24690\nin hex (two's complement) = FFFF9F8E\n37 in binary = 100101\n"),
            ("adv_08_jump_tables.asm", "20 + 5 = 25\n20 - 5 = 15\n20 * 5 = 100\n20 / 5 = 4\nmedium\nlarge\nout of range\n"),
        ];
        assert_eq!(super::advanced().len(), expected.len(), "an advanced lesson without an expected output here");
        let files = std::env::temp_dir().join(format!("atb_advanced_{}", std::process::id()));
        for (file, want) in expected {
            let src = super::source(file).unwrap_or_else(|| panic!("{file} not embedded"));
            let (p, d) = assemble(src);
            assert!(d.is_empty(), "{file}: {d:#?}");
            let mut m = Machine::new(Rc::new(p));
            m.files = crate::files::Sandbox::new(Some(files.clone()));
            for _ in 0..2_000_000 {
                m.step();
                if m.status != Status::Ready {
                    break;
                }
            }
            assert_eq!(m.status, Status::Exited(0), "{file}: {:?}", m.output);
            assert_eq!(m.output, *want, "{file}");
        }
        assert!(super::advanced().iter().all(|l| l.title.starts_with("Advanced ")));
        // adv_06 cleans up after itself.
        assert!(!files.join("data").exists());
        let _ = std::fs::remove_dir_all(&files);
    }
}

#[cfg(test)]
mod graphics_tests {
    use crate::asm::{assemble, Severity, SCREEN_BASE, SCREEN_W};
    use crate::cpu::{Machine, Status, StepEvent};
    use std::rc::Rc;

    fn load(file: &str) -> Machine {
        let (p, d) = assemble(super::source(file).unwrap_or_else(|| panic!("{file} not embedded")));
        assert!(d.iter().all(|d| d.severity != Severity::Error), "{file}: {d:#?}");
        assert!(d.is_empty(), "{file}: warnings {d:#?}");
        Machine::new(Rc::new(p))
    }

    fn pixel(m: &Machine, x: u32, y: u32) -> u8 {
        m.mem[(SCREEN_BASE + y * SCREEN_W + x) as usize]
    }

    /// Runs until `frames` wait_frame calls (or the program stops).
    fn run_frames(m: &mut Machine, frames: u32) {
        let mut seen = 0;
        for _ in 0..5_000_000 {
            match m.step() {
                StepEvent::Frame => {
                    seen += 1;
                    if seen == frames {
                        return;
                    }
                }
                StepEvent::Stopped => return,
                _ => {}
            }
        }
    }

    #[test]
    fn drawing() {
        let mut m = load("11_drawing.asm");
        run_frames(&mut m, 1);
        assert_eq!(m.status, Status::Exited(0));
        assert!(m.screen_used);
        assert_eq!(pixel(&m, 0, 0), 0); // stripe 0 = black
        assert_eq!(pixel(&m, 127, 5), 15); // last stripe = white
        assert_eq!(pixel(&m, 2, 50), 17); // background
        assert_eq!(pixel(&m, 8, 20), 16 + 2); // gradient corner
        assert_eq!(pixel(&m, 100, 40), 9); // red square
        assert_eq!(pixel(&m, 90, 26), 11); // yellow square
        assert_eq!(pixel(&m, 10, 102), 15); // line: y = 100 + 10/5
    }

    #[test]
    fn bouncing_ball_moves_and_bounces() {
        let mut m = load("12_bouncing_ball.asm");
        run_frames(&mut m, 1);
        assert_eq!(pixel(&m, 22, 51), 11); // ball after one move: x=22, y=51
        run_frames(&mut m, 300);
        assert_eq!(m.status, Status::Ready);
        assert_eq!(m.ticks, 301);
        let yellow = (0..128 * 128).filter(|i| m.mem[(SCREEN_BASE + i) as usize] == 11).count();
        assert_eq!(yellow, 36, "exactly one 6x6 ball on screen");
    }

    #[test]
    fn keyboard_moves_the_brush() {
        let mut m = load("13_keyboard.asm");
        run_frames(&mut m, 1);
        assert!(m.output.starts_with("Click the Screen"));
        m.key_event(129, true); // hold Right
        run_frames(&mut m, 10);
        m.key_event(129, false);
        assert_eq!(m.peek(m.prog.symbol("px").unwrap().value, 4), Some(72));
        assert_eq!(pixel(&m, 73, 63), 11); // painted in yellow along the way
        // Space picks a colour from the cube; C clears.
        m.key_event(32, true);
        m.key_event(32, false);
        run_frames(&mut m, 1);
        let c = m.peek(m.prog.symbol("colour").unwrap().value, 4).unwrap();
        assert!((16..232).contains(&c));
        m.key_event('C' as u32, true);
        run_frames(&mut m, 1);
        assert_eq!(pixel(&m, 10, 10), 235);
    }

    #[test]
    fn step_back_undoes_pixels() {
        let mut m = load("12_bouncing_ball.asm");
        run_frames(&mut m, 5);
        let before: Vec<u8> = m.mem.clone();
        let (steps, ticks) = (m.steps, m.ticks);
        run_frames(&mut m, 3);
        assert!(m.mem != before);
        while m.steps > steps {
            assert!(m.step_back());
        }
        assert!(m.mem == before, "memory (incl. screen) restored");
        assert_eq!(m.ticks, ticks);
    }

    #[test]
    fn game_of_life_blinker_blinks() {
        let mut m = load("15_game_of_life.asm");
        let cells = m.prog.symbol("cells").unwrap().value;
        let cell = |m: &Machine, x: u32, y: u32| m.mem[(cells + y * 32 + x) as usize];
        run_frames(&mut m, 1); // generation 0 drawn
        assert_eq!((cell(&m, 10, 10), cell(&m, 11, 10), cell(&m, 12, 10)), (1, 1, 1), "horizontal");
        assert_eq!(pixel(&m, 11 * 4 + 1, 10 * 4 + 1), 11, "drawn yellow");
        run_frames(&mut m, 1); // generation 1
        assert_eq!((cell(&m, 11, 9), cell(&m, 11, 10), cell(&m, 11, 11)), (1, 1, 1), "vertical");
        assert_eq!((cell(&m, 10, 10), cell(&m, 12, 10)), (0, 0));
        run_frames(&mut m, 4);
        assert_eq!(m.status, Status::Ready, "still running");
    }

    #[test]
    fn mandelbrot_draws_the_set() {
        let mut m = load("16_mandelbrot.asm");
        run_frames(&mut m, 1); // runs to the exit
        assert_eq!(m.status, Status::Exited(0));
        assert_eq!(pixel(&m, 64, 64), 0, "c = -0.5: inside the set, black");
        assert_ne!(pixel(&m, 0, 0), 0, "c = -2 - 1.5i: escapes, coloured");
    }

    #[test]
    fn big_factorial_is_exact() {
        // 50! computed here independently (base 10^9 limbs, as decimal text).
        let mut limbs: Vec<u64> = vec![1];
        for k in 2..=50u64 {
            let mut carry = 0;
            for l in limbs.iter_mut() {
                let v = *l * k + carry;
                *l = v % 1_000_000_000;
                carry = v / 1_000_000_000;
            }
            if carry > 0 {
                limbs.push(carry);
            }
        }
        let mut want = limbs.last().unwrap().to_string();
        for l in limbs.iter().rev().skip(1) {
            want.push_str(&format!("{l:09}"));
        }
        assert_eq!(want.len(), 65);
        let mut m = load("17_big_factorial.asm");
        run_frames(&mut m, 1);
        assert_eq!(m.status, Status::Exited(0));
        assert_eq!(m.output, format!("50! = {want}\n"));
    }

    #[test]
    fn round_ball_stays_inside_and_bounces() {
        let mut m = load("18_round_ball.asm");
        let sym = |m: &Machine, n: &str| m.peek(m.prog.symbol(n).unwrap().value, 4).unwrap() as i32;
        let r = 6;
        let (mut flips_x, mut flips_y) = (0, 0);
        let (mut vx, mut vy) = (sym(&m, "vx"), sym(&m, "vy"));
        for _ in 0..600 {
            run_frames(&mut m, 1);
            let (x, y) = (sym(&m, "x"), sym(&m, "y"));
            assert!((r..=127 - r).contains(&x) && (r..=127 - r).contains(&y), "centre ({x}, {y}) keeps the ball on the screen");
            assert_eq!(pixel(&m, x as u32, y as u32), 214, "the ball is drawn at its centre");
            assert_eq!(pixel(&m, (x + r) as u32, y as u32), 214, "round: the right edge pixel");
            if sym(&m, "vx") != vx {
                flips_x += 1;
                vx = sym(&m, "vx");
            }
            if sym(&m, "vy") != vy {
                flips_y += 1;
                vy = sym(&m, "vy");
            }
        }
        assert!(flips_x >= 2 && flips_y >= 2, "bounced off both sides ({flips_x}) and top / bottom ({flips_y})");
        // Only the ball and the background on the screen: erasing leaves no trail.
        let other = (0..128 * 128).filter(|&i| !matches!(m.mem[(SCREEN_BASE + i) as usize], 17 | 214)).count();
        assert_eq!(other, 0);
    }

    #[test]
    fn bouncing_line_redraws_cleanly() {
        let mut m = load("19_bouncing_line.asm");
        let pts = m.prog.symbol("pts").unwrap().value;
        let mut vx = m.peek(pts + 8, 4).unwrap();
        let mut bounces = 0;
        for _ in 0..300 {
            run_frames(&mut m, 1);
            for p in 0..2 {
                let x = m.peek(pts + p * 16, 4).unwrap() as i32;
                let y = m.peek(pts + p * 16 + 4, 4).unwrap() as i32;
                assert!((0..=127).contains(&x) && (0..=127).contains(&y), "point {p} at ({x}, {y})");
                assert_eq!(pixel(&m, x as u32, y as u32), 15, "point {p} drawn white");
            }
            let colours = |c: u8| (0..128 * 128).filter(|&i| m.mem[(SCREEN_BASE + i) as usize] == c).count();
            assert!(colours(14) > 0, "the line is drawn");
            assert_eq!(colours(17) + colours(14) + colours(15), 128 * 128, "nothing left over from earlier frames");
            if m.peek(pts + 8, 4).unwrap() != vx {
                bounces += 1;
                vx = m.peek(pts + 8, 4).unwrap();
            }
        }
        assert!(bounces >= 2, "point 0 bounced off the sides ({bounces})");
    }
}
