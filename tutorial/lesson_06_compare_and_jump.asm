; =============================================================
;  Lesson 6: Comparing and jumping (if / else)
;  You will learn: cmp, the flags, conditional jumps, and how to
;  build an if / else out of them.
; =============================================================
;
;  Assembly has no "if". Instead:
;     1. cmp a, b     computes a - b, throws the result away and
;                     only sets the FLAGS (ZF = result was zero,
;                     SF = negative, CF / OF = overflow, ...)
;     2. a CONDITIONAL JUMP looks at the flags and jumps to a
;        label if its condition holds; otherwise the program just
;        goes on with the next line.
;
;  The usual jumps after  cmp a, b :
;     je   a == b         jne  a != b
;     jl   a <  b         jle  a <= b        (signed numbers)
;     jg   a >  b         jge  a >= b
;     jb / ja / jbe / jae: the same for unsigned numbers
;  and  jmp  always jumps.
;
;  The pattern for  "if (a > b) X else Y" :
;          cmp   a, b
;          jle   else       ; jump over X when the condition FAILS
;          X
;          jmp   end
;     else:
;          Y
;     end:
; =============================================================

section .data
	t_bigger    db  " is bigger", 10, 0
	t_equal     db  "they are equal", 10, 0
	t_negative  db  " is negative", 10, 0
	t_positive  db  " is zero or positive", 10, 0

section .text
global _start

_start:
	; ---- which of two numbers is bigger? ----
	mov		esi, 17             ; a
	mov		edi, 25             ; b
	cmp		esi, edi
	je		equal               ; a == b
	jg		a_bigger            ; a > b
	mov		ebx, edi            ; else: b is bigger
	jmp		print_bigger
a_bigger:
	mov		ebx, esi
print_bigger:
	mov		eax, 200            ; print the bigger one
	int		0x80
	mov		eax, 202
	mov		ebx, t_bigger
	int		0x80
	jmp		sign_test
equal:
	mov		eax, 202
	mov		ebx, t_equal
	int		0x80

	; ---- is a number negative? ----
sign_test:
	mov		esi, -5
	mov		eax, 200
	mov		ebx, esi
	int		0x80
	cmp		esi, 0
	jl		negative            ; if (esi < 0)
	mov		ebx, t_positive     ; else
	jmp		print_sign
negative:
	mov		ebx, t_negative
print_sign:
	mov		eax, 202
	int		0x80

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; -------------------------------------------------------------
;  TRY IT
;   1. Step to  cmp esi, edi  and look at the Flags panel: it
;      lists which conditional jumps would be taken right now.
;   2. Change a and b: make them equal, swap them.
;   3. The difference between jl and jb: set esi to -5 and
;      replace  jl negative  with  jb negative . -5 is a huge
;      number when read as unsigned (0xFFFFFFFB), so jb is not
;      taken.
;   4. Print "even" or "odd" for a number (lesson 5: remainder).
;
;  Next: Lesson 7, loops.
; -------------------------------------------------------------
