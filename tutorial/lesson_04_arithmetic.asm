; =============================================================
;  Lesson 4: Adding and subtracting
;  You will learn: add, sub, inc, dec and neg, and that the
;  result always goes into the FIRST operand.
; =============================================================
;
;     add dst, src    dst = dst + src
;     sub dst, src    dst = dst - src
;     inc dst         dst = dst + 1
;     dec dst         dst = dst - 1
;     neg dst         dst = -dst
;
;  There is no "c = a + b" with three operands: you copy a into
;  a register first, then add b to it.
;
;  This program works on ESI step by step and prints each result:
;     esi = 10
;     esi = esi + 32      -> 42
;     esi = esi - 2       -> 40
;     esi = esi + 1       -> 41
;     esi = -esi          -> -41
;     esi + esi           -> -82
; =============================================================

section .text
global _start

_start:
	mov		esi, 10             ; esi = 10
	call	show                ; (show prints esi: see below)

	add		esi, 32             ; esi = 42
	call	show

	sub		esi, 2              ; esi = 40
	call	show

	inc		esi                 ; esi = 41
	call	show

	neg		esi                 ; esi = -41
	call	show

	mov		ecx, esi            ; registers can be added together too:
	add		ecx, esi            ; ecx = -41 + -41 = -82
	mov		esi, ecx
	call	show

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; show: prints the number in esi and a newline.
; (A small FUNCTION, so we don't repeat these lines every time.
;  Lesson 11 explains call and ret; for now just use it.)
show:
	mov		eax, 200            ; print_int(esi)
	mov		ebx, esi
	int		0x80
	mov		eax, 205            ; newline
	int		0x80
	ret

; -------------------------------------------------------------
;  TRY IT
;   1. Step with F10 (Step Over: runs "call show" as one step)
;      and watch ESI. Then try F11 (Step Into) on a call.
;   2. Press Shift+F10 (Step Back) to undo instructions.
;   3. Watch the flags panel: after  sub esi, 2  ZF (zero) is 0;
;      change it to  sub esi, 42  and ZF becomes 1.
;   4. Compute 100 - 58 + 7 in the same way.
;
;  Next: Lesson 5, multiplying and dividing.
; -------------------------------------------------------------
