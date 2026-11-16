; =============================================================
;  Lesson 15: Working with bits
;  You will learn: and, or, xor, not, shifts, and using them to
;  test, set and clear single bits.
; =============================================================
;
;  These work on each bit separately (1 = true, 0 = false):
;
;     and  a, b     1 only where BOTH are 1    -> clears bits
;     or   a, b     1 where EITHER is 1        -> sets bits
;     xor  a, b     1 where they DIFFER        -> flips bits
;     not  a        flips every bit
;     test a, b     an "and" that only sets the flags (like cmp)
;
;  Shifts move all bits left or right:
;     shl  a, n     left by n: a * 2^n   (fast multiply)
;     shr  a, n     right by n: a / 2^n  (unsigned divide)
;     sar  a, n     right, keeping the sign (signed divide)
;
;  A MASK picks the bits you care about:
;     and  eax, 0xFF      keeps the lowest 8 bits
;     or   eax, 1 << 3    sets bit 3 (1 << 3 = 8 = 0b1000)
;     test eax, 1         ZF = 0 if bit 0 is set (an odd number)
; =============================================================

section .data
	t_and   db  "0b1100 and 0b1010 = ", 0
	t_or    db  "0b1100 or  0b1010 = ", 0
	t_xor   db  "0b1100 xor 0b1010 = ", 0
	t_shl   db  "5 shl 3 = ", 0
	t_shr   db  "100 shr 2 = ", 0
	t_bits  db  "bits set in 0b10110111: ", 0
	t_odd   db  "13 is odd", 10, 0
	t_even  db  "13 is even", 10, 0

section .text
global _start

_start:
	mov		esi, 0b1100
	and		esi, 0b1010         ; 0b1000 = 8
	mov		ebx, t_and
	call	show

	mov		esi, 0b1100
	or		esi, 0b1010         ; 0b1110 = 14
	mov		ebx, t_or
	call	show

	mov		esi, 0b1100
	xor		esi, 0b1010         ; 0b0110 = 6
	mov		ebx, t_xor
	call	show

	mov		esi, 5
	shl		esi, 3              ; 5 * 8 = 40
	mov		ebx, t_shl
	call	show

	mov		esi, 100
	shr		esi, 2              ; 100 / 4 = 25
	mov		ebx, t_shr
	call	show

	; ---- count the 1 bits: test bit 0, shift right, repeat ----
	mov		eax, 0b10110111
	xor		esi, esi            ; count = 0
.count:
	test	eax, 1              ; is the lowest bit 1?
	jz		.zero
	inc		esi
.zero:
	shr		eax, 1              ; next bit into position 0
	jnz		.count              ; until no 1 bits are left
	mov		ebx, t_bits
	call	show                ; 6

	; ---- odd or even: just look at bit 0 ----
	mov		eax, 13
	test	eax, 1
	jz		.even
	mov		ebx, t_odd
	jmp		.print
.even:
	mov		ebx, t_even
.print:
	mov		eax, 202
	int		0x80

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; show: the text at ebx, then the number in esi.
show:
	mov		eax, 202
	int		0x80
	mov		eax, 200
	mov		ebx, esi
	int		0x80
	mov		eax, 205
	int		0x80
	ret

; -------------------------------------------------------------
;  TRY IT
;   1. Switch the Registers panel to "bin" and step: you can
;      see each bit change.
;   2. Hover a number like 0b10110111 in the code to see it in
;      decimal and hex.
;   3. xor a value with the same mask twice: you get the value
;      back. (That's why  xor eax, eax  sets eax to 0.)
;   4. Check whether a number is a power of two: n and (n - 1)
;      is 0 exactly then.
;
;  Next: Lesson 16, drawing on the screen.
; -------------------------------------------------------------
