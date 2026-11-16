; =============================================================
;  Lesson 5: Multiplying and dividing
;  You will learn: imul, idiv, the remainder in EDX, and why
;  cdq is needed before a division.
; =============================================================
;
;  MULTIPLY (signed):
;     imul dst, src          dst = dst * src
;     imul dst, src, number  dst = src * number
;
;  DIVIDE (signed) uses fixed registers, a leftover from the
;  first x86 CPUs:
;     idiv src     divides the 64-bit number EDX:EAX by src
;                  EAX = quotient, EDX = remainder
;
;  Because the dividend is EDX:EAX (EDX = the upper 32 bits),
;  EDX must be set up first. cdq does that: it fills EDX with
;  the sign of EAX (0 for positive, all ones for negative).
;  Forgetting cdq is the classic bug: you get a wrong result or
;  a "division overflow" fault.
;
;  (mul / div are the unsigned versions; they need EDX = 0.)
; =============================================================

section .data
	t_mul   db  "7 * 6 = ", 0
	t_div   db  "47 / 5 = ", 0
	t_rem   db  " remainder ", 0
	t_neg   db  "-47 / 5 = ", 0

section .text
global _start

_start:
	; ---- 7 * 6 ----
	mov		esi, 7
	imul	esi, 6              ; esi = 42
	mov		ebx, t_mul
	call	print_text
	mov		ebx, esi
	call	print_number
	call	newline

	; ---- 47 / 5 ----
	mov		eax, 47             ; the dividend goes into eax
	cdq                         ; edx = 0 (47 is positive)
	mov		ecx, 5              ; the divisor (any register or memory)
	idiv	ecx                 ; eax = 9, edx = 2
	mov		esi, eax            ; keep the results: printing uses eax
	mov		edi, edx
	mov		ebx, t_div
	call	print_text
	mov		ebx, esi
	call	print_number
	mov		ebx, t_rem
	call	print_text
	mov		ebx, edi
	call	print_number
	call	newline

	; ---- -47 / 5: signed division rounds toward zero ----
	mov		eax, -47
	cdq                         ; edx = 0xFFFFFFFF (eax is negative)
	mov		ecx, 5
	idiv	ecx                 ; eax = -9, edx = -2
	mov		esi, eax
	mov		edi, edx
	mov		ebx, t_neg
	call	print_text
	mov		ebx, esi
	call	print_number
	mov		ebx, t_rem
	call	print_text
	mov		ebx, edi
	call	print_number
	call	newline

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; Small helpers (lesson 11 explains functions).
print_text:                     ; prints the 0-terminated text at ebx
	mov		eax, 202
	int		0x80
	ret
print_number:                   ; prints ebx as a signed number
	mov		eax, 200
	int		0x80
	ret
newline:
	mov		eax, 205
	int		0x80
	ret

; -------------------------------------------------------------
;  TRY IT
;   1. Replace the first cdq with  mov edx, 1 . The CPU now
;      divides EDX:EAX = 4294967343 by 5 and prints 858993468:
;      a wrong answer and no error. With  mov edx, 5  the
;      quotient doesn't even fit in 32 bits: the CPU stops with
;      a division overflow fault.
;   2. Divide by zero (mov ecx, 0). Another fault.
;   3. A number is even when  number / 2  has remainder 0.
;      Check whether 1234 is even.
;
;  Next: Lesson 6, comparing and jumping.
; -------------------------------------------------------------
