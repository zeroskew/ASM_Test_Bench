; =============================================================
;  Sum of 1..N with a loop
;  Shows: counters, add, the 'loop' instruction, and printing
;  numbers with the ASM Test Bench helper system calls.
; =============================================================

section .data
	N       equ 10
	text    db  "Sum of 1..10 = ", 0

section .text
global _start

_start:
	xor		eax, eax        ; eax = running total (0)
	mov		ecx, N          ; ecx = counter, counts down to 0

add_next:
	add		eax, ecx        ; total += counter
	loop	add_next        ; ecx--, jump back while ecx != 0

	mov		esi, eax        ; keep the result safe in esi

	mov		eax, 202        ; print_string (ASM Test Bench helper)
	mov		ebx, text
	int		0x80

	mov		eax, 200        ; print_int (ASM Test Bench helper)
	mov		ebx, esi
	int		0x80

	mov		eax, 205        ; print_newline
	int		0x80

	mov		eax, 1          ; exit(0)
	xor		ebx, ebx
	int		0x80
