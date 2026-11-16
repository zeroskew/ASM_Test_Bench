; =============================================================
;  Lesson 1: Your first program
;  You will learn: what an instruction is, what a register is,
;  and how a program ends.
; =============================================================
;
;  An assembly program is a list of INSTRUCTIONS. The CPU runs
;  them one after another, from top to bottom.
;
;  The CPU keeps the numbers it works with in REGISTERS: small,
;  very fast storage cells inside the CPU. A 32-bit x86 CPU has
;  eight general-purpose ones: EAX, EBX, ECX, EDX, ESI, EDI, EBP
;  and ESP. Each holds one 32-bit number.
;
;  This program does only one thing: it asks the system to end
;  the program. To do that it puts two numbers into registers
;  and then "calls the kernel" with  int 0x80 :
;     EAX = which service we want (1 = exit)
;     EBX = the exit code (0 means "everything went fine")
;
;  HOW TO WATCH IT:
;     Press F10 to run one instruction at a time. Look at the
;     Registers panel on the right: the register an instruction
;     changes lights up.
; =============================================================

section .text           ; the code goes into the .text section
global _start           ; _start is where the program begins

_start:                 ; a LABEL: a name for this place in the code
	mov		eax, 1      ; mov = copy. eax = 1   (service: exit)
	mov		ebx, 0      ; ebx = 0   (exit code)
	int		0x80        ; ask the kernel to do it: the program ends

; -------------------------------------------------------------
;  TRY IT
;   1. Change the exit code to 42 and run it (F5). The status
;      bar at the bottom shows the exit code.
;   2. Swap the two mov lines. Does it still work? Why?
;   3. Delete the int 0x80 line and run again. The program now
;      runs "past the end": the simulator stops it with a fault.
;
;  Next: Lesson 2 shows how to print text.
; -------------------------------------------------------------
