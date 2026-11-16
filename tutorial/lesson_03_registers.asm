; =============================================================
;  Lesson 3: Registers and numbers
;  You will learn: mov with registers and numbers, the ASM Test
;  Bench helpers for printing numbers, and the 8/16-bit parts of
;  a register.
; =============================================================
;
;  mov dst, src   copies src into dst. src can be a number or
;  another register; dst is a register (or memory, lesson 8).
;
;  Printing numbers with the write call would mean converting
;  them to text first. To keep things simple, ASM Test Bench has
;  helper system calls (numbers 200 and up, not real Linux):
;     EAX = 200  print_int      prints EBX as a signed number
;     EAX = 202  print_string   prints the 0-terminated text at EBX
;     EAX = 204  print_hex      prints EBX in hexadecimal
;     EAX = 205  print_newline
;
;  Every 32-bit register has smaller parts with their own names:
;
;     EAX  [ 32 bits                                ]
;      AX                      [ 16 bits            ]
;      AH                      [ 8 bits ]
;      AL                                [ 8 bits ]
;
;  (the same for EBX/BX/BH/BL, ECX/CX/CH/CL, EDX/DX/DH/DL)
; =============================================================

section .data
	t_copy  db  "esi = ", 0           ; 0 marks the end for print_string
	t_hex   db  "eax in hex = ", 0
	t_al    db  "after mov al, 0xFF: eax = ", 0

section .text
global _start

_start:
	mov		ecx, 1234           ; ecx = 1234
	mov		esi, ecx            ; copy a register: esi = 1234 too

	mov		eax, 202            ; print_string(t_copy)
	mov		ebx, t_copy         ; (ebx = the ADDRESS of the text)
	int		0x80
	mov		eax, 200            ; print_int(ebx)
	mov		ebx, esi            ; ebx = 1234
	int		0x80
	mov		eax, 205            ; newline
	int		0x80

	; Numbers can be written in several bases:
	;   1234 (decimal), 0x4D2 or 4D2h (hex), 0b10011010010 (binary)
	mov		eax, 0x12345678

	mov		esi, eax            ; keep the value: printing needs eax
	mov		eax, 202
	mov		ebx, t_hex
	int		0x80
	mov		eax, 204            ; print_hex(ebx)
	mov		ebx, esi
	int		0x80
	mov		eax, 205
	int		0x80

	; Writing to AL changes only the lowest 8 bits of EAX.
	mov		eax, esi            ; eax = 0x12345678
	mov		al, 0xFF            ; eax = 0x123456FF
	mov		esi, eax
	mov		eax, 202
	mov		ebx, t_al
	int		0x80
	mov		eax, 204
	mov		ebx, esi
	int		0x80
	mov		eax, 205
	int		0x80

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; -------------------------------------------------------------
;  TRY IT
;   1. Step with F10 and switch the Registers panel between
;      hex, dec and bin (top right of the panel).
;   2. Hover EAX in the code while paused: you see its value
;      and the values of AX, AH and AL.
;   3. Change  mov al, 0xFF  to  mov ah, 0xFF . What is printed?
;   4. Try  mov al, 300 . The assembler complains: 300 doesn't
;      fit in 8 bits (0..255).
;
;  Next: Lesson 4, arithmetic.
; -------------------------------------------------------------
