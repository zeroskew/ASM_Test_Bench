; =============================================================
;  Lesson 7: Loops
;  You will learn: how a loop is a jump back to an earlier
;  label, counting loops, and the loop instruction.
; =============================================================
;
;  A loop is a conditional jump that goes BACKWARDS:
;
;          mov   ecx, 1           ; i = 1
;     top:
;          ...body...
;          inc   ecx              ; i++
;          cmp   ecx, 5
;          jle   top              ; while (i <= 5) go again
;
;  Labels that start with a dot (.next) are LOCAL: they belong
;  to the label above them, so every function can have its own
;  .loop / .done without name clashes.
;
;  loop label   is a shortcut for "dec ecx, jump if ecx != 0".
;  It counts ECX down to zero.
; =============================================================

section .data
	t_sum   db  "sum of 1..10 = ", 0
	t_down  db  "countdown: ", 0

section .text
global _start

_start:
	; ---- print 1 2 3 4 5 ----
	mov		esi, 1              ; i = 1
.next:
	mov		eax, 200            ; print i
	mov		ebx, esi
	int		0x80
	mov		eax, 201            ; print_char(' ')
	mov		ebx, ' '
	int		0x80
	inc		esi                 ; i++
	cmp		esi, 5
	jle		.next               ; while i <= 5
	mov		eax, 205
	int		0x80

	; ---- sum = 1 + 2 + ... + 10 ----
	xor		edi, edi            ; sum = 0  (xor of a register with
								; itself is the usual way to zero it)
	mov		esi, 1              ; i = 1
.add:
	add		edi, esi            ; sum += i
	inc		esi
	cmp		esi, 10
	jle		.add
	mov		eax, 202
	mov		ebx, t_sum
	int		0x80
	mov		eax, 200
	mov		ebx, edi            ; 55
	int		0x80
	mov		eax, 205
	int		0x80

	; ---- countdown with the loop instruction ----
	mov		eax, 202
	mov		ebx, t_down
	int		0x80
	mov		ecx, 5              ; loop counts ecx down
.down:
	mov		eax, 200
	mov		ebx, ecx
	int		0x80                ; (system calls leave ecx alone)
	mov		eax, 201
	mov		ebx, ' '
	int		0x80
	loop	.down               ; ecx--, jump while ecx != 0
	mov		eax, 205
	int		0x80

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; -------------------------------------------------------------
;  TRY IT
;   1. View > "Show execution counts": the editor shows how many
;      times each line ran. The loop bodies ran 5 and 10 times.
;   2. Put a breakpoint (click left of the line number, or F9)
;      on  add edi, esi  and press F5 repeatedly: each press
;      runs one more trip around the loop.
;   3. Make the second loop add up 1..100 (it's 5050).
;   4. Print the multiplication table of 7 (7 14 21 ... 70).
;
;  Next: Lesson 8, variables in memory.
; -------------------------------------------------------------
