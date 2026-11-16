; =============================================================
;  Print a number in binary
;  Shows: shifts, the carry flag, building output one character
;  at a time. Hover a number in the code to see it in hex/binary.
; =============================================================

section .text
global _start

_start:
	mov		edx, 2026       ; the number to print
	mov		ecx, 32         ; 32 bits
	xor		edi, edi        ; edi = 1 once we've seen the first 1-bit

.bit:
	shl		edx, 1          ; top bit -> CF
	jc		.one
	test	edi, edi        ; skip leading zeros
	jz		.next
	mov		bl, '0'
	jmp		.print
.one:
	mov		edi, 1
	mov		bl, '1'
.print:
	mov		eax, 201        ; print_char(bl)
	int		0x80
.next:
	loop	.bit

	mov		eax, 205
	int		0x80
	mov		eax, 1
	xor		ebx, ebx
	int		0x80
