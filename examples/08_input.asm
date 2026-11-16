; =============================================================
;  Console input
;  Shows: reading text and numbers. When the program waits for
;  input, type into the Console box at the bottom and press Enter.
; =============================================================

section .data
	ask_name    db  "What is your name? ", 0
	hello       db  "Nice to meet you, ", 0
	ask_num     db  "Enter a number: ", 0
	squared     db  "Its square is ", 0

section .bss
	name        resb 32

section .text
global _start

_start:
	mov		eax, 202
	mov		ebx, ask_name
	int		0x80

	mov		eax, 3              ; read(0, name, 31)
	mov		ebx, 0
	mov		ecx, name
	mov		edx, 31
	int		0x80                ; eax = bytes read (with the newline)

	mov		eax, 202
	mov		ebx, hello
	int		0x80
	mov		eax, 202            ; name still ends with its newline
	mov		ebx, name
	int		0x80

	mov		eax, 202
	mov		ebx, ask_num
	int		0x80
	mov		eax, 203            ; read_int -> eax
	int		0x80
	imul	eax, eax
	mov		esi, eax

	mov		eax, 202
	mov		ebx, squared
	int		0x80
	mov		eax, 200
	mov		ebx, esi
	int		0x80
	mov		eax, 205
	int		0x80

	mov		eax, 1
	xor		ebx, ebx
	int		0x80
