; =============================================================
;  Lesson 2: Printing text
;  You will learn: the .data section, defining text with db,
;  computing its length with equ, and the write system call.
; =============================================================
;
;  Text is just bytes in memory: one byte per character, using
;  the ASCII code ('H' = 72, 'e' = 101, ...). We put the text in
;  the .data section and give it a name (a label).
;
;     db  = "define bytes": puts these bytes into memory
;     10  = the newline character (moves to the next line)
;     equ = defines a constant. $ means "the current address",
;           so  $ - msg  is "here minus the start of msg" =
;           the number of bytes in msg.
;
;  The write system call (EAX = 4) needs three arguments:
;     EBX = where to write (1 = the console)
;     ECX = the ADDRESS of the first byte
;     EDX = how many bytes
; =============================================================

section .data
	msg     db  "Hello, assembly!", 10
	len     equ $ - msg             ; = 17 (16 characters + newline)

section .text
global _start

_start:
	mov		eax, 4          ; service 4: write
	mov		ebx, 1          ; to the console
	mov		ecx, msg        ; the address of the text
	mov		edx, len        ; its length
	int		0x80            ; -> prints the text

	mov		eax, 1          ; service 1: exit
	mov		ebx, 0
	int		0x80

; -------------------------------------------------------------
;  TRY IT
;   1. Hover the mouse over "msg" or "len" in the code: the
;      editor shows the address and the value.
;   2. Open the Memory tab (bottom panel) after starting with
;      F10 and find the text at address 0x4000.
;   3. Set edx to 5 instead of len. Only "Hello" is printed:
;      write prints exactly as many bytes as you ask for.
;   4. Add a second message and print it too.
;
;  Next: Lesson 3, registers and numbers.
; -------------------------------------------------------------
