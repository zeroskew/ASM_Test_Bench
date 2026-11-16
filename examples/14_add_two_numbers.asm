; =============================================================
;  Add two numbers
;  Shows: the simplest way to read input. The program asks for
;  two numbers and prints their sum. When it waits, type a
;  number into the Console box at the bottom and press Enter.
; =============================================================

section .data
	ask1    db  "First number: ", 0
	ask2    db  "Second number: ", 0
	result  db  "Sum: ", 0

section .text
global _start

_start:
	mov		eax, 202            ; print_string
	mov		ebx, ask1
	int		0x80
	mov		eax, 203            ; read_int: the number goes into EAX
	int		0x80
	mov		esi, eax            ; keep it in ESI

	mov		eax, 202
	mov		ebx, ask2
	int		0x80
	mov		eax, 203            ; second number -> EAX
	int		0x80
	add		esi, eax            ; ESI = first + second

	mov		eax, 202
	mov		ebx, result
	int		0x80
	mov		eax, 200            ; print_int
	mov		ebx, esi
	int		0x80
	mov		eax, 205            ; print_newline
	int		0x80

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80
