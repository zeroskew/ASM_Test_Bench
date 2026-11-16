//! Built-in reference: instructions, registers, flags and system calls.

pub struct Doc {
    pub name: &'static str,
    pub group: &'static str,
    pub syntax: &'static str,
    pub summary: &'static str,
    pub flags: &'static str,
    pub example: &'static str,
}

const fn d(name: &'static str, group: &'static str, syntax: &'static str, summary: &'static str, flags: &'static str, example: &'static str) -> Doc {
    Doc { name, group, syntax, summary, flags, example }
}

pub const INSTRUCTIONS: &[Doc] = &[
    d("mov", "Data", "mov dst, src", "Copies src into dst. Both must be the same size; at most one can be memory.", "none", "mov eax, 5\nmov [count], eax"),
    d("movzx", "Data", "movzx reg, r/m8|r/m16", "Copies a smaller value into a bigger register, filling the upper bits with zeros (unsigned).", "none", "movzx eax, byte [msg]"),
    d("movsx", "Data", "movsx reg, r/m8|r/m16", "Copies a smaller value into a bigger register, copying the sign bit into the upper bits (signed).", "none", "movsx eax, byte [delta]"),
    d("lea", "Data", "lea reg, [address]", "Load Effective Address: computes the address expression and stores it (no memory is read). Often used for arithmetic.", "none", "lea esi, [array + ecx*4]\nlea eax, [eax + eax*2]   ; eax *= 3"),
    d("xchg", "Data", "xchg a, b", "Swaps the values of a and b.", "none", "xchg eax, ebx"),
    d("bswap", "Data", "bswap reg32", "Reverses the byte order of a 32-bit register (little endian <-> big endian).", "none", "mov eax, 0x11223344\nbswap eax   ; 0x44332211"),
    d("xadd", "Data", "xadd dst, reg", "Exchange and add: reg = the old dst, dst = dst + reg.", "OF SF ZF PF CF", "xadd [counter], eax"),
    d("cmpxchg", "Data", "cmpxchg dst, reg", "Compare and exchange: if EAX (AX / AL) = dst then dst = reg and ZF = 1, else EAX = dst and ZF = 0.", "OF SF ZF PF CF", "mov eax, 0\nmov ecx, 1\ncmpxchg [lock], ecx"),
    d("xlat", "Data", "xlat  (= xlatb)", "Table lookup: AL = [EBX + AL].", "none", "mov ebx, hex_digits\nmov al, 11\nxlat   ; al = 'B'"),
    d("push", "Stack", "push src", "Decrements ESP by 4 (or 2) and stores src at [ESP]. The stack grows downwards.", "none", "push eax\npush 10"),
    d("pop", "Stack", "pop dst", "Loads [ESP] into dst, then increments ESP by 4 (or 2).", "none", "pop ebx"),
    d("pushad", "Stack", "pushad", "Pushes EAX, ECX, EDX, EBX, (original) ESP, EBP, ESI, EDI.", "none", "pushad"),
    d("popad", "Stack", "popad", "Pops EDI, ESI, EBP, (skip ESP), EBX, EDX, ECX, EAX. Reverses pushad.", "none", "popad"),
    d("pushfd", "Stack", "pushfd", "Pushes the EFLAGS register.", "none", "pushfd"),
    d("popfd", "Stack", "popfd", "Pops a value into EFLAGS.", "all", "popfd"),
    d("add", "Arithmetic", "add dst, src", "dst = dst + src", "OF SF ZF PF CF", "add eax, 10"),
    d("adc", "Arithmetic", "adc dst, src", "Add with carry: dst = dst + src + CF. Used for multi-word addition.", "OF SF ZF PF CF", "add eax, ebx\nadc edx, ecx"),
    d("sub", "Arithmetic", "sub dst, src", "dst = dst - src", "OF SF ZF PF CF", "sub esp, 8"),
    d("sbb", "Arithmetic", "sbb dst, src", "Subtract with borrow: dst = dst - src - CF.", "OF SF ZF PF CF", "sbb edx, ecx"),
    d("inc", "Arithmetic", "inc dst", "dst = dst + 1 (CF is not changed).", "OF SF ZF PF", "inc ecx"),
    d("dec", "Arithmetic", "dec dst", "dst = dst - 1 (CF is not changed).", "OF SF ZF PF", "dec ecx\njnz loop"),
    d("neg", "Arithmetic", "neg dst", "Two's complement negation: dst = 0 - dst.", "OF SF ZF PF CF", "neg eax"),
    d("cmp", "Arithmetic", "cmp a, b", "Computes a - b and sets the flags, but throws the result away. Follow it with a conditional jump.", "OF SF ZF PF CF", "cmp eax, 10\njl smaller"),
    d("mul", "Arithmetic", "mul src", "Unsigned multiply. 8-bit: AX = AL*src. 16-bit: DX:AX = AX*src. 32-bit: EDX:EAX = EAX*src.", "CF OF (set if the high half is non-zero)", "mov eax, 6\nmov ebx, 7\nmul ebx      ; eax = 42, edx = 0"),
    d("imul", "Arithmetic", "imul src | imul reg, r/m | imul reg, r/m, imm", "Signed multiply. The one-operand form works like mul. The 2/3-operand forms keep only the low half: reg = reg*r/m or reg = r/m*imm.", "CF OF (set if the result was truncated)", "imul eax, ebx\nimul ecx, [n], 10"),
    d("div", "Arithmetic", "div src", "Unsigned divide. 32-bit: EAX = EDX:EAX / src, EDX = remainder. 8-bit: AL = AX / src, AH = remainder. Clear EDX first!", "undefined", "xor edx, edx\nmov ecx, 10\ndiv ecx      ; eax = quotient, edx = remainder"),
    d("idiv", "Arithmetic", "idiv src", "Signed divide (like div). Sign-extend EAX into EDX first with cdq.", "undefined", "cdq\nidiv ecx"),
    d("cdq", "Arithmetic", "cdq", "Sign-extends EAX into EDX:EAX (EDX = 0 or 0xFFFFFFFF). Use before idiv.", "none", "cdq"),
    d("cwd", "Arithmetic", "cwd", "Sign-extends AX into DX:AX.", "none", "cwd"),
    d("cbw", "Arithmetic", "cbw", "Sign-extends AL into AX.", "none", "cbw"),
    d("cwde", "Arithmetic", "cwde", "Sign-extends AX into EAX.", "none", "cwde"),
    d("and", "Logic", "and dst, src", "Bitwise AND. Commonly used to clear bits (masking).", "SF ZF PF; CF=OF=0", "and eax, 0xFF   ; keep the low byte"),
    d("or", "Logic", "or dst, src", "Bitwise OR. Commonly used to set bits.", "SF ZF PF; CF=OF=0", "or al, 0x20     ; to lowercase"),
    d("xor", "Logic", "xor dst, src", "Bitwise exclusive OR. 'xor reg, reg' is the classic way to set a register to 0.", "SF ZF PF; CF=OF=0", "xor eax, eax"),
    d("not", "Logic", "not dst", "Inverts every bit (one's complement).", "none", "not eax"),
    d("test", "Logic", "test a, b", "Computes a AND b, sets the flags, and throws the result away.", "SF ZF PF; CF=OF=0", "test eax, eax\njz is_zero"),
    d("bt", "Logic", "bt value, bit", "Bit test: copies bit number 'bit' of value into CF (bit is a register or a constant). With memory and a register, the number can go past 31: memory is then a bit array (bit 100 is in the dword at +12).", "CF", "bt eax, 3\njc bit3_is_set"),
    d("bts", "Logic", "bts value, bit", "Bit test and set: CF = the bit, then the bit becomes 1.", "CF", "bts dword [flags], 5"),
    d("btr", "Logic", "btr value, bit", "Bit test and reset: CF = the bit, then the bit becomes 0.", "CF", "btr eax, 0"),
    d("btc", "Logic", "btc value, bit", "Bit test and complement: CF = the bit, then the bit is flipped.", "CF", "btc eax, 7"),
    d("bsf", "Logic", "bsf dst, src", "Bit scan forward: dst = the number of the lowest 1 bit of src. If src is 0: ZF = 1 and dst is unchanged.", "ZF", "bsf ecx, eax   ; eax = 0b1000 -> ecx = 3"),
    d("bsr", "Logic", "bsr dst, src", "Bit scan reverse: dst = the number of the highest 1 bit of src (floor of log2). If src is 0: ZF = 1.", "ZF", "bsr ecx, eax   ; eax = 1000 -> ecx = 9"),
    d("popcnt", "Logic", "popcnt dst, src", "Population count: dst = how many bits of src are 1.", "ZF; CF=OF=SF=PF=0", "popcnt eax, ebx"),
    d("shl", "Shift", "shl dst, count|cl", "Shift left (same as sal). Each shift multiplies by 2. The last bit shifted out goes to CF.", "CF OF SF ZF PF", "shl eax, 2      ; eax *= 4"),
    d("sal", "Shift", "sal dst, count|cl", "Arithmetic shift left; identical to shl.", "CF OF SF ZF PF", "sal eax, 1"),
    d("shr", "Shift", "shr dst, count|cl", "Logical shift right; fills with zeros. Unsigned divide by 2^count.", "CF OF SF ZF PF", "shr eax, 1"),
    d("sar", "Shift", "sar dst, count|cl", "Arithmetic shift right; copies the sign bit. Signed divide by 2^count (rounding down).", "CF SF ZF PF; OF=0", "sar eax, 1"),
    d("rol", "Shift", "rol dst, count|cl", "Rotate left: bits leaving on the left come back on the right.", "CF OF", "rol eax, 8"),
    d("rcl", "Shift", "rcl dst, count|cl", "Rotate left through carry: CF acts as an extra bit above the value (the old CF comes in on the right, the top bit goes into CF). Used for shifting numbers bigger than 32 bits.", "CF OF", "shl eax, 1\nrcl edx, 1   ; edx:eax shifted left by 1"),
    d("rcr", "Shift", "rcr dst, count|cl", "Rotate right through carry: the old CF comes in on the left, the lowest bit goes into CF.", "CF OF", "shr edx, 1\nrcr eax, 1   ; edx:eax shifted right by 1"),
    d("shld", "Shift", "shld dst, src, count|cl", "Double shift left: dst is shifted left and the free bits are filled from the top of src (src doesn't change).", "CF OF SF ZF PF", "shld edx, eax, 4   ; edx:eax << 4 (upper half)"),
    d("shrd", "Shift", "shrd dst, src, count|cl", "Double shift right: dst is shifted right and the free bits are filled from the bottom of src.", "CF OF SF ZF PF", "shrd eax, edx, 4   ; edx:eax >> 4 (lower half)"),
    d("ror", "Shift", "ror dst, count|cl", "Rotate right.", "CF OF", "ror al, 4"),
    d("jmp", "Control", "jmp label | jmp reg | jmp [mem]", "Unconditional jump.", "none", "jmp done"),
    d("je", "Control", "je label  (= jz)", "Jump if equal / zero (ZF=1).", "none", "cmp eax, ebx\nje same"),
    d("jne", "Control", "jne label  (= jnz)", "Jump if not equal / not zero (ZF=0).", "none", "dec ecx\njnz again"),
    d("jg", "Control", "jg label  (= jnle)", "Jump if greater (signed): ZF=0 and SF=OF.", "none", "cmp eax, 0\njg positive"),
    d("jge", "Control", "jge label  (= jnl)", "Jump if greater or equal (signed): SF=OF.", "none", "jge ok"),
    d("jl", "Control", "jl label  (= jnge)", "Jump if less (signed): SF≠OF.", "none", "cmp eax, 0\njl negative"),
    d("jle", "Control", "jle label  (= jng)", "Jump if less or equal (signed): ZF=1 or SF≠OF.", "none", "jle done"),
    d("ja", "Control", "ja label  (= jnbe)", "Jump if above (unsigned): CF=0 and ZF=0.", "none", "cmp al, 'z'\nja not_lower"),
    d("jae", "Control", "jae label  (= jnb, jnc)", "Jump if above or equal (unsigned): CF=0.", "none", "jae ok"),
    d("jb", "Control", "jb label  (= jnae, jc)", "Jump if below (unsigned): CF=1.", "none", "cmp al, 'a'\njb not_lower"),
    d("jbe", "Control", "jbe label  (= jna)", "Jump if below or equal (unsigned): CF=1 or ZF=1.", "none", "jbe small"),
    d("js", "Control", "js label", "Jump if sign (negative result): SF=1.", "none", "js negative"),
    d("jns", "Control", "jns label", "Jump if not sign: SF=0.", "none", "jns positive"),
    d("jo", "Control", "jo label", "Jump if overflow (signed overflow): OF=1.", "none", "jo overflowed"),
    d("jno", "Control", "jno label", "Jump if no overflow: OF=0.", "none", "jno fine"),
    d("jc", "Control", "jc label", "Jump if carry: CF=1.", "none", "jc carried"),
    d("jnc", "Control", "jnc label", "Jump if no carry: CF=0.", "none", "jnc fine"),
    d("jz", "Control", "jz label", "Jump if zero (ZF=1). Same as je.", "none", "test eax, eax\njz zero"),
    d("jnz", "Control", "jnz label", "Jump if not zero (ZF=0). Same as jne.", "none", "jnz loop"),
    d("jecxz", "Control", "jecxz label", "Jump if ECX is 0 (flags not used).", "none", "jecxz skip"),
    d("loop", "Control", "loop label", "ECX = ECX - 1; jump if ECX ≠ 0. A counted loop in one instruction.", "none", "mov ecx, 10\nagain:\n  ...\n  loop again"),
    d("loope", "Control", "loope label  (= loopz)", "ECX = ECX - 1; jump if ECX ≠ 0 and ZF = 1: loop while equal (e.g. searching while bytes match).", "none", "mov ecx, 10\nscan:\n  ...\n  cmp al, ' '\n  loope scan"),
    d("loopne", "Control", "loopne label  (= loopnz)", "ECX = ECX - 1; jump if ECX ≠ 0 and ZF = 0: loop until something is found.", "none", "next:\n  lodsb\n  cmp al, '.'\n  loopne next"),
    d("enter", "Stack", "enter size, 0", "Sets up a stack frame: push ebp / mov ebp, esp / sub esp, size (room for locals). The partner of leave.", "none", "f:\n  enter 8, 0     ; 8 bytes of locals\n  ...\n  leave\n  ret"),
    d("call", "Control", "call label | call reg", "Pushes the return address (the next instruction) and jumps to the function.", "none", "push 5\ncall square\nadd esp, 4"),
    d("ret", "Control", "ret | ret n", "Pops the return address into EIP. 'ret n' also removes n bytes of arguments (stdcall).", "none", "ret"),
    d("leave", "Control", "leave", "Undoes a stack frame: mov esp, ebp / pop ebp.", "none", "leave\nret"),
    d("int", "Control", "int 0x80 | int3", "Software interrupt. 'int 0x80' performs a system call (EAX = call number). 'int3' pauses the debugger here.", "none", "mov eax, 1\nmov ebx, 0\nint 0x80"),
    d("nop", "Control", "nop", "Does nothing.", "none", "nop"),
    d("hlt", "Control", "hlt", "Stops the program (exit code 0).", "none", "hlt"),
    d("sete", "Conditional", "setCC r/m8   (sete, setne, setl, setg, seta, setb, ...)", "Sets a byte to 1 if the condition holds, else 0.", "none", "cmp eax, ebx\nsetl al"),
    d("cmove", "Conditional", "cmovCC reg, r/m   (cmove, cmovne, cmovl, cmovg, ...)", "Conditional move: copies src into reg only if the condition holds.", "none", "cmp eax, ebx\ncmovl eax, ebx   ; eax = max(eax, ebx)"),
    d("clc", "Flags", "clc", "Clears the carry flag (CF=0).", "CF", "clc"),
    d("stc", "Flags", "stc", "Sets the carry flag (CF=1).", "CF", "stc"),
    d("cmc", "Flags", "cmc", "Complements (flips) the carry flag.", "CF", "cmc"),
    d("lahf", "Flags", "lahf", "Copies the low flags into AH: bit 7 SF, 6 ZF, 2 PF, 0 CF.", "none", "lahf"),
    d("sahf", "Flags", "sahf", "Copies AH into the low flags (SF, ZF, PF, CF): the opposite of lahf.", "SF ZF PF CF", "sahf"),
    d("cld", "Flags", "cld", "Clears the direction flag: string instructions move forwards (ESI/EDI increase).", "DF", "cld"),
    d("std", "Flags", "std", "Sets the direction flag: string instructions move backwards.", "DF", "std"),
    d("movsb", "String", "[rep] movsb | movsw | movsd", "Copies [ESI] to [EDI], then advances both. With rep: repeats ECX times (memcpy).", "none", "mov esi, src\nmov edi, dst\nmov ecx, len\ncld\nrep movsb"),
    d("stosb", "String", "[rep] stosb | stosw | stosd", "Stores AL/AX/EAX at [EDI], then advances EDI. With rep: fills ECX elements (memset).", "none", "xor eax, eax\nmov edi, buf\nmov ecx, 16\nrep stosb"),
    d("lodsb", "String", "lodsb | lodsw | lodsd", "Loads [ESI] into AL/AX/EAX, then advances ESI.", "none", "next:\n  lodsb\n  test al, al\n  jnz next"),
    d("cmpsb", "String", "[repe|repne] cmpsb | cmpsw | cmpsd", "Compares [ESI] with [EDI] (like cmp), then advances both. With repe: compare while equal.", "OF SF ZF PF CF", "repe cmpsb"),
    d("scasb", "String", "[repe|repne] scasb | scasw | scasd", "Compares AL/AX/EAX with [EDI], then advances EDI. 'repne scasb' searches for AL.", "OF SF ZF PF CF", "mov al, 0\nrepne scasb   ; find the terminating 0"),
];

pub struct RegDoc {
    pub name: &'static str,
    pub summary: &'static str,
}

pub const REGISTERS: &[RegDoc] = &[
    RegDoc { name: "eax", summary: "Accumulator. Return values, mul/div results, system call number." },
    RegDoc { name: "ebx", summary: "Base register. General purpose; first system call argument." },
    RegDoc { name: "ecx", summary: "Counter. Used by loop, rep, and shifts by cl; second system call argument." },
    RegDoc { name: "edx", summary: "Data register. High half of mul/div; third system call argument." },
    RegDoc { name: "esi", summary: "Source index. Source pointer for string instructions." },
    RegDoc { name: "edi", summary: "Destination index. Destination pointer for string instructions." },
    RegDoc { name: "ebp", summary: "Base pointer. Points to the current stack frame: [ebp+8] = first argument, [ebp-4] = first local." },
    RegDoc { name: "esp", summary: "Stack pointer. Address of the top of the stack; push/pop/call/ret change it." },
    RegDoc { name: "eip", summary: "Instruction pointer. Address of the next instruction to execute." },
];

pub struct FlagDoc {
    pub name: &'static str,
    pub bit: u32,
    pub title: &'static str,
    pub summary: &'static str,
}

pub const FLAGS: &[FlagDoc] = &[
    FlagDoc { name: "CF", bit: crate::cpu::CF, title: "Carry", summary: "Unsigned overflow: the result didn't fit (or a borrow happened). Used by jb/jae/jc." },
    FlagDoc { name: "ZF", bit: crate::cpu::ZF, title: "Zero", summary: "The result was zero. Used by je/jne/jz/jnz." },
    FlagDoc { name: "SF", bit: crate::cpu::SF, title: "Sign", summary: "The result is negative (its top bit is 1)." },
    FlagDoc { name: "OF", bit: crate::cpu::OF, title: "Overflow", summary: "Signed overflow: the result has the wrong sign. Used by jl/jg/jo." },
    FlagDoc { name: "PF", bit: crate::cpu::PF, title: "Parity", summary: "The low byte of the result has an even number of 1 bits." },
    FlagDoc { name: "DF", bit: crate::cpu::DF, title: "Direction", summary: "String instructions go backwards when set (std) and forwards when clear (cld)." },
];

pub struct SyscallDoc {
    pub eax: u32,
    pub name: &'static str,
    pub args: &'static str,
    pub summary: &'static str,
}

pub const SYSCALLS: &[SyscallDoc] = &[
    SyscallDoc { eax: 1, name: "exit", args: "ebx = exit code", summary: "Ends the program." },
    SyscallDoc { eax: 3, name: "read", args: "ebx = fd, ecx = buffer, edx = max bytes", summary: "fd 0: reads one line of console input (including the newline). A file (fd from open): reads up to edx bytes. eax = bytes read (0 at the end of a file), or -errno." },
    SyscallDoc { eax: 4, name: "write", args: "ebx = fd, ecx = address, edx = length", summary: "fd 1 / 2: writes edx bytes from ecx to the console. A file (fd from open): writes them to the file. eax = bytes written, or -errno." },
    SyscallDoc { eax: 5, name: "open", args: "ebx = file name (0-terminated), ecx = flags", summary: "Opens a file in the program's folder (File > Open Files Folder). flags: 0 read, 1 write, 2 both; + 0x40 create, 0x80 must not exist, 0x200 empty it, 0x400 append. eax = fd (3 and up), or -errno (-2 = not found)." },
    SyscallDoc { eax: 6, name: "close", args: "ebx = fd", summary: "Closes a file. eax = 0, or -9 (EBADF) if it wasn't open." },
    SyscallDoc { eax: 8, name: "creat", args: "ebx = file name", summary: "Creates a file (or empties an existing one) and opens it for writing. eax = fd, or -errno." },
    SyscallDoc { eax: 10, name: "unlink", args: "ebx = file name", summary: "Deletes a file. eax = 0, or -errno." },
    SyscallDoc { eax: 13, name: "time", args: "ebx = 0, or the address of a dword", summary: "eax = seconds since 1 January 1970 (also stored at ebx if it isn't 0)." },
    SyscallDoc { eax: 19, name: "lseek", args: "ebx = fd, ecx = offset, edx = 0 from the start / 1 from here / 2 from the end", summary: "Moves the read / write position of a file. eax = the new position (lseek(fd, 0, 2) = the file size), or -errno." },
    SyscallDoc { eax: 20, name: "getpid", args: "-", summary: "eax = the process id (always 1 here)." },
    SyscallDoc { eax: 38, name: "rename", args: "ebx = old name, ecx = new name", summary: "Renames or moves a file or folder. eax = 0, or -errno." },
    SyscallDoc { eax: 39, name: "mkdir", args: "ebx = folder name", summary: "Creates a folder. eax = 0, or -errno (-17 = it already exists)." },
    SyscallDoc { eax: 40, name: "rmdir", args: "ebx = folder name", summary: "Deletes an empty folder. eax = 0, or -errno (-39 = not empty)." },
    SyscallDoc { eax: 45, name: "brk", args: "ebx = new end of the heap, or 0", summary: "The heap starts right after .data / .bss. brk(0) returns where it ends now; brk(address) moves the end (up to the screen memory at 0xC000) and returns the new end." },
    SyscallDoc { eax: 200, name: "print_int", args: "ebx = value", summary: "ASM Test Bench helper: prints ebx as a signed decimal number." },
    SyscallDoc { eax: 201, name: "print_char", args: "bl = character", summary: "ASM Test Bench helper: prints one character." },
    SyscallDoc { eax: 202, name: "print_string", args: "ebx = address of a 0-terminated string", summary: "ASM Test Bench helper: prints a C-style string." },
    SyscallDoc { eax: 203, name: "read_int", args: "-", summary: "ASM Test Bench helper: reads a line and parses it as a number into eax." },
    SyscallDoc { eax: 204, name: "print_hex", args: "ebx = value", summary: "ASM Test Bench helper: prints ebx as 0x%08X." },
    SyscallDoc { eax: 205, name: "print_newline", args: "-", summary: "ASM Test Bench helper: prints a line break." },
    SyscallDoc { eax: 206, name: "print_uint", args: "ebx = value", summary: "ASM Test Bench helper: prints ebx as an unsigned decimal number." },
    SyscallDoc { eax: 210, name: "clear_screen", args: "ebx = colour", summary: "Screen: fills the whole 128x128 screen with one palette colour." },
    SyscallDoc { eax: 211, name: "set_pixel", args: "ebx = x, ecx = y, edx = colour", summary: "Screen: sets one pixel (off-screen coordinates are ignored). Same as mov byte [SCREEN + y*SCREEN_W + x], colour." },
    SyscallDoc { eax: 212, name: "fill_rect", args: "ebx = x, ecx = y, edx = width, esi = height, edi = colour", summary: "Screen: fills a rectangle, clipped to the screen." },
    SyscallDoc { eax: 213, name: "wait_frame", args: "-", summary: "Screen: waits for the next display frame (~60 per second) so animations run at a steady speed." },
    SyscallDoc { eax: 214, name: "read_key", args: "-", summary: "Keyboard: eax = the next key pressed in the Screen window (KEY_LEFT, 'A', ...), or 0 if none. Doesn't wait." },
    SyscallDoc { eax: 215, name: "key_down", args: "ebx = key code", summary: "Keyboard: eax = 1 while that key is held down, else 0." },
    SyscallDoc { eax: 216, name: "random", args: "ebx = n", summary: "eax = a random number from 0 to n-1 (any 32-bit number if n = 0)." },
    SyscallDoc { eax: 217, name: "get_ticks", args: "-", summary: "eax = the number of frames waited for so far (a clock for animations)." },
];

/// Doc entry for a mnemonic (condition-code aliases resolve to their family entry).
pub fn lookup(mnemonic: &str) -> Option<&'static Doc> {
    let m = mnemonic.to_ascii_lowercase();
    if let Some(d) = INSTRUCTIONS.iter().find(|d| d.name == m) {
        return Some(d);
    }
    let alias = match m.as_str() {
        "jnae" => "jb",
        "jnb" => "jae",
        "jnbe" => "ja",
        "jna" => "jbe",
        "jnl" => "jge",
        "jnge" => "jl",
        "jnle" => "jg",
        "jng" => "jle",
        "pusha" => "pushad",
        "popa" => "popad",
        "pushf" => "pushfd",
        "loopz" => "loope",
        "loopnz" => "loopne",
        "xlatb" => "xlat",
        "popf" => "popfd",
        "int3" => "int",
        "movsw" | "movsd" => "movsb",
        "stosw" | "stosd" => "stosb",
        "lodsw" | "lodsd" => "lodsb",
        "cmpsw" | "cmpsd" => "cmpsb",
        "scasw" | "scasd" => "scasb",
        _ if m.starts_with("set") => "sete",
        _ if m.starts_with("cmov") => "cmove",
        _ if m.starts_with('j') => "jne",
        _ => return None,
    };
    INSTRUCTIONS.iter().find(|d| d.name == alias)
}

pub fn register(name: &str) -> Option<&'static RegDoc> {
    let n = name.to_ascii_lowercase();
    let full = match n.as_str() {
        "ax" | "ah" | "al" => "eax",
        "bx" | "bh" | "bl" => "ebx",
        "cx" | "ch" | "cl" => "ecx",
        "dx" | "dh" | "dl" => "edx",
        "si" => "esi",
        "di" => "edi",
        "bp" => "ebp",
        "sp" => "esp",
        other => other,
    };
    REGISTERS.iter().find(|r| r.name == full)
}
