; =============================================================
;  Fibonacci numbers
;  Shows: keeping two values in registers, xchg, saving a
;  register on the stack. Set a breakpoint (click the gutter)
;  on 'add' and press F5 repeatedly to watch the sequence grow.
; =============================================================

section .data
	sep     db  ", ", 0

section .text
global _start

_start:
	mov		esi, 0          ; a = F(0)
	mov		edi, 1          ; b = F(1)
	mov		ecx, 15         ; how many numbers to print

print_next:
	push	ecx             ; keep the counter safe while printing

	mov		eax, 200        ; print_int(a)
	mov		ebx, esi
	int		0x80

	pop		ecx
	cmp		ecx, 1
	je		finished        ; no separator after the last one

	mov		eax, 202        ; print_string(", ")
	mov		ebx, sep
	int		0x80

	add		esi, edi        ; a = a + b
	xchg	esi, edi        ; swap: now a = old b, b = old a + b
	loop	print_next

finished:
	mov		eax, 205
	int		0x80
	mov		eax, 1
	xor		ebx, ebx
	int		0x80
