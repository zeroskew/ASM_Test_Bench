; =============================================================
;  Lesson 10: The stack
;  You will learn: push and pop, the ESP register, and the
;  "last in, first out" order.
; =============================================================
;
;  The STACK is a part of memory the program uses as a pile of
;  dwords. ESP (the stack pointer) holds the address of the top
;  item. The stack grows DOWNWARDS, toward smaller addresses:
;
;     push x     esp = esp - 4, then [esp] = x
;     pop  r     r = [esp], then esp = esp + 4
;
;  Whatever was pushed last comes off first, like a stack of
;  plates. That makes it handy for:
;     - keeping a register's value while you need the register
;       for something else (push it, use it, pop it back),
;     - reversing things,
;     - function calls (lessons 11 and 12).
;
;  Every push needs a matching pop (or esp has to be put back
;  some other way), or things go wrong later.
; =============================================================

section .data
	t_rev   db  "reversed: ", 0
	t_swap  db  "after the swap: eax = ", 0
	t_ebx   db  ", ebx = ", 0

section .text
global _start

_start:
	; ---- push 1..5, pop them back: they come out reversed ----
	mov		ecx, 1
.push:
	push	ecx                 ; 1, 2, 3, 4, 5 onto the stack
	inc		ecx
	cmp		ecx, 5
	jle		.push

	mov		eax, 202
	mov		ebx, t_rev
	int		0x80
	mov		esi, 5              ; pop 5 times
.pop:
	pop		ebx                 ; 5 first, 1 last
	mov		eax, 200
	int		0x80
	mov		eax, 201
	mov		ebx, ' '
	int		0x80
	dec		esi
	jnz		.pop                ; jnz: jump if the result wasn't zero
	mov		eax, 205
	int		0x80

	; ---- swap two registers through the stack ----
	mov		eax, 111
	mov		ebx, 222
	push	eax
	push	ebx
	pop		eax                 ; eax = 222 (pushed last)
	pop		ebx                 ; ebx = 111

	mov		esi, eax
	mov		edi, ebx
	mov		eax, 202
	mov		ebx, t_swap
	int		0x80
	mov		eax, 200
	mov		ebx, esi
	int		0x80
	mov		eax, 202
	mov		ebx, t_ebx
	int		0x80
	mov		eax, 200
	mov		ebx, edi
	int		0x80
	mov		eax, 205
	int		0x80

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; -------------------------------------------------------------
;  TRY IT
;   1. Step through the first loop and watch the Stack panel
;      on the right: each push adds an entry and ESP goes down
;      by 4. The ESP marker shows the top.
;   2. Step Back (Shift+F10) over a pop: the value goes back on.
;   3. Remove one pop from the second part. eax and ebx are now
;      wrong, and one value stays on the stack forever.
;   4. Push the digits of 1234 (1234 / 10 gives the last digit
;      as the remainder) and pop them to print 4 3 2 1.
;
;  Next: Lesson 11, functions.
; -------------------------------------------------------------
