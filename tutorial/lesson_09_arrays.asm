; =============================================================
;  Lesson 9: Arrays
;  You will learn: arrays are values next to each other in
;  memory, and how to reach element i with [base + i*4].
; =============================================================
;
;  dd 5, 8, 3  puts three dwords one after another:
;
;     address:   scores+0   scores+4   scores+8
;     value:        5          8          3
;
;  Element i is at  scores + i*4  (4 = the size of a dword).
;  x86 can compute that inside the brackets:
;
;     mov   eax, [scores + ecx*4]
;
;  The general form is  [base + index*scale + displacement]
;  with scale 1, 2, 4 or 8. For a byte array, use *1 (or just
;  [array + ecx]).
;
;  The number of elements: ($ - scores) / 4 right after the
;  data, so it stays correct when you add or remove values.
; =============================================================

section .data
	scores  dd  72, 95, 61, 88, 79, 100, 54, 91
	n       equ ($ - scores) / 4        ; 8 elements
	t_list  db  "scores: ", 0
	t_sum   db  "sum: ", 0
	t_avg   db  "average: ", 0
	t_best  db  "best: ", 0

section .text
global _start

_start:
	; ---- print all elements ----
	mov		eax, 202
	mov		ebx, t_list
	int		0x80
	xor		ecx, ecx                    ; i = 0
.print:
	mov		eax, 200
	mov		ebx, [scores + ecx*4]       ; scores[i]
	int		0x80
	mov		eax, 201
	mov		ebx, ' '
	int		0x80
	inc		ecx
	cmp		ecx, n
	jl		.print                      ; while i < n
	mov		eax, 205
	int		0x80

	; ---- sum, average and best in one pass ----
	xor		esi, esi                    ; sum = 0
	mov		edi, [scores]               ; best = scores[0]
	xor		ecx, ecx                    ; i = 0
.scan:
	mov		eax, [scores + ecx*4]
	add		esi, eax                    ; sum += scores[i]
	cmp		eax, edi
	jle		.not_best
	mov		edi, eax                    ; best = scores[i]
.not_best:
	inc		ecx
	cmp		ecx, n
	jl		.scan

	mov		eax, 202
	mov		ebx, t_sum
	int		0x80
	mov		eax, 200
	mov		ebx, esi                    ; 640
	int		0x80
	mov		eax, 205
	int		0x80

	mov		eax, esi                    ; average = sum / n
	cdq
	mov		ecx, n
	idiv	ecx
	mov		esi, eax
	mov		eax, 202
	mov		ebx, t_avg
	int		0x80
	mov		eax, 200
	mov		ebx, esi                    ; 80
	int		0x80
	mov		eax, 205
	int		0x80

	mov		eax, 202
	mov		ebx, t_best
	int		0x80
	mov		eax, 200
	mov		ebx, edi                    ; 100
	int		0x80
	mov		eax, 205
	int		0x80

	mov		eax, 1                      ; exit(0)
	mov		ebx, 0
	int		0x80

; -------------------------------------------------------------
;  TRY IT
;   1. Find the array in the Memory tab: 8 groups of 4 bytes.
;      Hover  scores  in the code to see its address.
;   2. Add more scores: n, the sum and the average follow.
;   3. Also find the worst score.
;   4. Change  ecx*4  into  ecx*2 . Every value is now read
;      from the wrong place: the scale must match the size.
;
;  Next: Lesson 10, the stack.
; -------------------------------------------------------------
