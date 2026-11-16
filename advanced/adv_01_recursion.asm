; =============================================================
;  Advanced 1: Recursion (the Towers of Hanoi)
;  You will learn: functions that call themselves, why every
;  call needs its own stack frame, and how the stack unwinds.
; =============================================================
;
;  The puzzle: move a tower of N disks from peg A to peg C, one
;  disk at a time, never putting a bigger disk on a smaller one.
;  The recursive idea:
;
;     hanoi(n, from, to, via):
;         if n == 0: done
;         hanoi(n-1, from, via, to)    ; move the n-1 disks out of the way
;         move disk n: from -> to
;         hanoi(n-1, via, to, from)    ; put them back on top
;
;  Each call gets its own ARGUMENTS on the stack ([ebp+8] ...),
;  so the copies don't overwrite each other: the stack keeps all
;  the calls that are "in progress". With 3 disks the deepest
;  point has 4 frames of hanoi on the stack.
;
;  WATCH: put a breakpoint (F9) on "inc dword [moves]" and press
;  F5 repeatedly. The Call Stack panel shows how deep you are, the
;  Stack panel shows each frame's arguments.
; =============================================================

section .data
	t_move  db  "Move disk ", 0
	t_from  db  " from ", 0
	t_to    db  " to ", 0
	t_total db  "Moves: ", 0

section .bss
	moves   resd 1

section .text
global main

main:
	push	'B'                 ; via   (arguments: last one first)
	push	'C'                 ; to
	push	'A'                 ; from
	push	3                   ; n = 3 disks
	call	hanoi
	add		esp, 16             ; remove the 4 arguments

	mov		eax, 202
	mov		ebx, t_total
	int		0x80
	mov		eax, 200
	mov		ebx, [moves]        ; 2^3 - 1 = 7
	int		0x80
	mov		eax, 205
	int		0x80
	xor		eax, eax            ; return 0
	ret

; ---- hanoi(n, from, to, via) ----
;   [ebp+8] = n, [ebp+12] = from, [ebp+16] = to, [ebp+20] = via
hanoi:
	push	ebp
	mov		ebp, esp
	mov		eax, [ebp+8]
	cmp		eax, 0
	je		.done               ; no disks: nothing to do

	; hanoi(n-1, from, via, to)
	push	dword [ebp+16]      ; via  = our "to"
	push	dword [ebp+20]      ; to   = our "via"
	push	dword [ebp+12]      ; from = our "from"
	dec		eax
	push	eax                 ; n-1
	call	hanoi
	add		esp, 16

	call	print_move          ; move disk n: from -> to
	inc		dword [moves]

	; hanoi(n-1, via, to, from)
	push	dword [ebp+12]      ; via  = our "from"
	push	dword [ebp+16]      ; to   = our "to"
	push	dword [ebp+20]      ; from = our "via"
	mov		eax, [ebp+8]
	dec		eax
	push	eax
	call	hanoi
	add		esp, 16
.done:
	pop		ebp
	ret

; Prints "Move disk n from X to Y" using the caller's frame (ebp).
print_move:
	mov		eax, 202
	mov		ebx, t_move
	int		0x80
	mov		eax, 200
	mov		ebx, [ebp+8]        ; n
	int		0x80
	mov		eax, 202
	mov		ebx, t_from
	int		0x80
	mov		eax, 201
	mov		ebx, [ebp+12]       ; from
	int		0x80
	mov		eax, 202
	mov		ebx, t_to
	int		0x80
	mov		eax, 201
	mov		ebx, [ebp+16]       ; to
	int		0x80
	mov		eax, 205
	int		0x80
	ret

; -------------------------------------------------------------
;  TRY IT
;   1. Change 3 to 5 disks: 31 moves. (2^n - 1: the number of
;      moves doubles with every disk.)
;   2. Try 20 disks without the printing. How many steps does the
;      status bar show? Recursion can be expensive.
;   3. print_move reads [ebp+8] although it has no frame of its
;      own: it uses its caller's EBP. Add  push ebp / mov ebp, esp
;      to print_move and see what breaks.
;   4. Write recursive fibonacci: fib(n) = fib(n-1) + fib(n-2).
; -------------------------------------------------------------
