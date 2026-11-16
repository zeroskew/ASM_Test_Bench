; =============================================================
;  Flags playground
;  Step through (F10) and watch the flag chips on the right:
;  CF = unsigned overflow, OF = signed overflow,
;  ZF = result is zero, SF = result is negative.
; =============================================================

section .text
global _start

_start:
	mov		al, 250
	add		al, 10          ; 260 doesn't fit in a byte: CF=1, al=4

	mov		al, 127
	add		al, 1           ; 127+1 = -128 as signed: OF=1, SF=1

	mov		eax, 5
	sub		eax, 5          ; zero: ZF=1

	mov		eax, 3
	sub		eax, 5          ; -2: SF=1, and CF=1 (3 < 5 unsigned)

	mov		eax, -1
	cmp		eax, 1          ; signed: -1 < 1 ...
	setl	bl              ; bl = 1
	setb	bh              ; ... but unsigned 0xFFFFFFFF > 1: bh = 0

	mov		eax, 0x80000000
	shl		eax, 1          ; the top bit goes into CF, eax = 0, ZF=1

	mov		eax, 1
	xor		ebx, ebx
	int		0x80
