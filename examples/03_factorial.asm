; =============================================================
;  Recursive factorial
;  Shows: functions, call/ret, stack frames (ebp), arguments on
;  the stack. Watch the Stack and Call Stack panels while you
;  step INTO (F11) the calls, and step OUT (Shift+F11).
; =============================================================

section .text
global main

; main is "called" by the system, so it can end with 'ret'
; (the exit code is the value in eax).
main:
	push	6               ; argument: n = 6
	call	factorial       ; eax = 6!
	add		esp, 4          ; remove the argument

	mov		ebx, eax
	mov		eax, 200        ; print_int
	int		0x80
	mov		eax, 205        ; newline
	int		0x80

	xor		eax, eax        ; return 0
	ret

; int factorial(int n)
factorial:
	push	ebp             ; save the caller's frame pointer
	mov		ebp, esp        ; [ebp+8] = n, [ebp+4] = return address

	mov		eax, [ebp+8]    ; eax = n
	cmp		eax, 1
	jle		.done           ; if n <= 1: return n

	dec		eax
	push	eax             ; argument: n - 1
	call	factorial       ; eax = (n-1)!
	add		esp, 4

	imul	eax, [ebp+8]    ; eax = n * (n-1)!

.done:
	pop		ebp             ; restore the caller's frame
	ret
