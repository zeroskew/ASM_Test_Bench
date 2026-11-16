; =============================================================
;  Lesson 14: Reading input
;  You will learn: reading numbers with read_int, reading text
;  with the read system call, and using what the user typed.
; =============================================================
;
;  When the program waits for input, the Console (bottom panel)
;  shows it: type into the box at the bottom and press Enter.
;  (You can also type the input before running: it is queued
;  until the program reads it.)
;
;     EAX = 203  read_int     reads a line, EAX = its number
;                             (ASM Test Bench helper)
;     EAX = 3    read         reads a line of text:
;                             EBX = 0 (keyboard), ECX = buffer,
;                             EDX = room in the buffer;
;                             returns EAX = bytes read, including
;                             the Enter (newline) at the end
;
;  The buffer for text goes into .bss.
; =============================================================

section .data
	ask_a   db  "First number: ", 0
	ask_b   db  "Second number: ", 0
	t_max   db  "The bigger one is ", 0
	t_sum   db  "Their sum is ", 0
	ask_n   db  "Your name: ", 0
	t_hi    db  "Hello, ", 0
	t_end   db  "!", 10, 0

section .bss
	name    resb 32

section .text
global _start

_start:
	; ---- two numbers ----
	mov		eax, 202
	mov		ebx, ask_a
	int		0x80
	mov		eax, 203            ; read_int
	int		0x80
	mov		esi, eax            ; a

	mov		eax, 202
	mov		ebx, ask_b
	int		0x80
	mov		eax, 203
	int		0x80
	mov		edi, eax            ; b

	mov		eax, 202
	mov		ebx, t_sum
	int		0x80
	mov		ebx, esi
	add		ebx, edi            ; a + b
	mov		eax, 200
	int		0x80
	mov		eax, 205
	int		0x80

	mov		eax, 202
	mov		ebx, t_max
	int		0x80
	mov		ebx, esi            ; max(a, b) (lesson 6)
	cmp		esi, edi
	jge		.print_max
	mov		ebx, edi
.print_max:
	mov		eax, 200
	int		0x80
	mov		eax, 205
	int		0x80

	; ---- a line of text ----
	mov		eax, 202
	mov		ebx, ask_n
	int		0x80
	mov		eax, 3              ; read(0, name, 31)
	mov		ebx, 0
	mov		ecx, name
	mov		edx, 31             ; leave room for a 0 at the end
	int		0x80                ; eax = bytes read
	; Replace the newline at the end with a 0, so the name can
	; be printed in the middle of a sentence.
	cmp		eax, 0
	je		.no_name
	mov		byte [name + eax - 1], 0
.no_name:
	mov		eax, 202
	mov		ebx, t_hi
	int		0x80
	mov		eax, 202
	mov		ebx, name
	int		0x80
	mov		eax, 202
	mov		ebx, t_end
	int		0x80

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; -------------------------------------------------------------
;  TRY IT
;   1. Run it (F5) and answer in the Console.
;   2. Type the three answers into the Console box BEFORE you
;      press F5 (Enter after each): they are queued.
;   3. Enter negative numbers, or the same number twice.
;   4. Ask for a number n and print 1..n (lesson 7).
;   5. Look at  name  in the Memory tab after the read: the
;      characters you typed, and the 0 that replaced Enter.
;
;  Next: Lesson 15, working with bits.
; -------------------------------------------------------------
