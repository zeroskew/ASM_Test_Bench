; =============================================================
;  50 factorial, exactly
;  Shows: big numbers (arbitrary precision): an array of "limbs"
;  in base 1,000,000,000, multiplying it with mul / div and a
;  carry, and printing the limbs with leading zeros.
; =============================================================
;
;  50! has 65 digits, far more than 32 bits (about 9 digits) can
;  hold. So the number is stored as limbs of 9 decimal digits,
;  lowest first: 123456789012 = limbs [456789012, 123].
;  Multiplying by k is long multiplication:
;     for each limb:  edx:eax = limb * k + carry
;                     limb  = that mod 10^9,  carry = that / 10^9
;  Printing: the top limb as it is, every other limb as exactly 9
;  digits (zeros in front).

BASE    equ 1000000000
N       equ 50

section .data
	t_head  db  "50! = ", 0

section .bss
	limbs   resd 16             ; up to 144 digits
	count   resd 1              ; limbs in use
	digits9 resb 9

section .text
global _start

_start:
	mov		dword [limbs], 1    ; the number = 1
	mov		dword [count], 1
	mov		ebx, 2              ; k = 2 .. N
.multiply:
	xor		ecx, ecx            ; limb index
	xor		esi, esi            ; carry
.limb:
	mov		eax, [limbs + ecx*4]
	mul		ebx                 ; edx:eax = limb * k
	add		eax, esi            ; + carry (64-bit add)
	adc		edx, 0
	mov		edi, BASE
	div		edi                 ; eax = carry, edx = the new limb
	mov		[limbs + ecx*4], edx
	mov		esi, eax
	inc		ecx
	cmp		ecx, [count]
	jl		.limb
	test	esi, esi            ; a carry left: the number got longer
	jz		.next_k
	mov		[limbs + ecx*4], esi
	inc		dword [count]
.next_k:
	inc		ebx
	cmp		ebx, N
	jle		.multiply

	; ---- print: top limb, then the others with 9 digits each ----
	mov		eax, 202
	mov		ebx, t_head
	int		0x80
	mov		ecx, [count]
	dec		ecx
	mov		eax, 200
	mov		ebx, [limbs + ecx*4]
	int		0x80
.lower:
	dec		ecx
	js		.done               ; below limb 0: finished
	mov		eax, [limbs + ecx*4]
	call	print9
	jmp		.lower
.done:
	mov		eax, 205
	int		0x80
	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; print9: eax as exactly 9 digits (leading zeros). Keeps ecx.
print9:
	push	ecx
	mov		edi, 9
	mov		ebx, 10
.digit:
	xor		edx, edx
	div		ebx                 ; edx = the last digit
	add		dl, '0'
	dec		edi
	mov		[digits9 + edi], dl
	test	edi, edi
	jnz		.digit
	mov		eax, 4              ; write(1, digits9, 9)
	mov		ebx, 1
	mov		ecx, digits9
	mov		edx, 9
	int		0x80
	pop		ecx
	ret
