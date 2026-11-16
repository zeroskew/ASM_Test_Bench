; =============================================================
;  Largest element of an array
;  Shows: arrays in memory, addressing with [base + index*4],
;  signed comparisons (jle), and the Memory view (bottom panel).
; =============================================================

section .data
	numbers dd  17, -4, 42, 8, 23, 15, -100, 39
	count   equ ($ - numbers) / 4      ; bytes / 4 = elements
	text    db  "Maximum: ", 0

section .text
global _start

_start:
	mov		eax, [numbers]          ; max = numbers[0]
	mov		ecx, 1                  ; i = 1

next:
	cmp		ecx, count
	jge		done                    ; while i < count
	mov		edx, [numbers + ecx*4]  ; edx = numbers[i]
	cmp		edx, eax
	jle		not_bigger
	mov		eax, edx                ; max = numbers[i]
not_bigger:
	inc		ecx
	jmp		next

done:
	mov		esi, eax
	mov		eax, 202
	mov		ebx, text
	int		0x80
	mov		eax, 200
	mov		ebx, esi
	int		0x80
	mov		eax, 205
	int		0x80

	mov		eax, 1
	xor		ebx, ebx
	int		0x80
