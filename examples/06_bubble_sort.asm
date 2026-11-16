; =============================================================
;  Bubble sort
;  Shows: nested loops, swapping values in memory, functions
;  that save registers (pushad/popad). Open the Memory tab and
;  click 'arr' to watch the values move (changed bytes glow).
; =============================================================

section .data
	arr     dd  9, 3, 7, 1, 8, 2, 6, 5, 4
	n       equ ($ - arr) / 4
	space   db  " ", 0

section .text
global _start

_start:
	call	sort
	call	print_array
	mov		eax, 1
	xor		ebx, ebx
	int		0x80

; Sorts 'arr' in place (ascending).
sort:
	pushad
	mov		ecx, n - 1              ; outer: n-1 passes
.outer:
	xor		esi, esi                ; i = 0
.inner:
	mov		eax, [arr + esi*4]
	mov		ebx, [arr + esi*4 + 4]
	cmp		eax, ebx
	jle		.no_swap
	mov		[arr + esi*4], ebx      ; swap arr[i] and arr[i+1]
	mov		[arr + esi*4 + 4], eax
.no_swap:
	inc		esi
	cmp		esi, ecx
	jl		.inner
	loop	.outer
	popad
	ret

; Prints the array separated by spaces.
print_array:
	pushad
	xor		esi, esi
.next:
	mov		eax, 200
	mov		ebx, [arr + esi*4]
	int		0x80
	mov		eax, 202
	mov		ebx, space
	int		0x80
	inc		esi
	cmp		esi, n
	jl		.next
	mov		eax, 205
	int		0x80
	popad
	ret
