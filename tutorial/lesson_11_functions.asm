; =============================================================
;  Lesson 11: Functions
;  You will learn: call and ret, passing arguments and results
;  in registers, and keeping registers safe inside a function.
; =============================================================
;
;  A FUNCTION is a piece of code you can run from many places.
;
;     call  name   pushes the address of the next instruction
;                  (the RETURN ADDRESS) and jumps to name
;     ret          pops that address and jumps back to it
;
;  So a function is just a label that ends with ret, and the
;  stack remembers where to come back to.
;
;  A common convention (and the one used here):
;     - arguments go in registers (here: EBX, ECX),
;     - the result comes back in EAX,
;     - a function that uses other registers saves them with
;       push at the start and pop before ret, so the caller's
;       values survive.
; =============================================================

section .data
	t_sq    db  "square(9) = ", 0
	t_max   db  "max(-3, 7) = ", 0
	t_pow   db  "power(2, 10) = ", 0

section .text
global _start

_start:
	mov		ebx, t_sq
	call	print_text
	mov		ebx, 9
	call	square              ; eax = 81
	mov		ebx, eax
	call	print_line_number

	mov		ebx, t_max
	call	print_text
	mov		ebx, -3
	mov		ecx, 7
	call	max                 ; eax = 7
	mov		ebx, eax
	call	print_line_number

	mov		ebx, t_pow
	call	print_text
	mov		ebx, 2
	mov		ecx, 10
	call	power               ; eax = 1024
	mov		ebx, eax
	call	print_line_number

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; ---- eax = ebx * ebx ----
square:
	mov		eax, ebx
	imul	eax, ebx
	ret

; ---- eax = the bigger of ebx and ecx ----
max:
	mov		eax, ebx
	cmp		eax, ecx
	jge		.done
	mov		eax, ecx
.done:
	ret

; ---- eax = ebx to the power ecx (ecx >= 0) ----
; Uses ecx as a counter, so it saves and restores it.
power:
	push	ecx                 ; save the caller's ecx
	mov		eax, 1
.again:
	cmp		ecx, 0
	je		.done
	imul	eax, ebx
	dec		ecx
	jmp		.again
.done:
	pop		ecx                 ; restore it
	ret

; ---- helpers: functions can call functions ----
print_text:                     ; the 0-terminated text at ebx
	mov		eax, 202
	int		0x80
	ret

print_line_number:              ; ebx as a number, then a newline
	mov		eax, 200
	int		0x80
	call	newline             ; a call inside a function
	ret

newline:
	mov		eax, 205
	int		0x80
	ret

; -------------------------------------------------------------
;  TRY IT
;   1. Step INTO a call (F11) and look at the Stack panel: the
;      return address is labelled. The Call Stack panel lists
;      the functions you are in.
;   2. Step OUT (Shift+F11) to finish the function and return.
;   3. Write  cube  (ebx * ebx * ebx) using square.
;   4. Remove  ret  at the end of square. The CPU just runs on
;      into max: labels are only names, not walls.
;
;  Next: Lesson 12, arguments on the stack and stack frames.
; -------------------------------------------------------------
