; =============================================================
;  Advanced 7: Numbers <-> text without the helpers
;  You will learn: how print_int and read_int work inside:
;  turning a number into digits (divide by the base, the
;  remainders are the digits backwards), and text into a number
;  (multiply by 10 and add). Only the real write call is used.
; =============================================================
;
;  NUMBER -> TEXT (itoa), e.g. 1234 in base 10:
;     1234 / 10 = 123 remainder 4
;      123 / 10 =  12 remainder 3
;       12 / 10 =   1 remainder 2
;        1 / 10 =   0 remainder 1      -> digits 4 3 2 1, backwards
;  so we fill a buffer from its END towards the start.
;  The same works for any base; the digit characters come from a
;  table "0123456789ABCDEF" (xlat looks them up).
;
;  TEXT -> NUMBER (atoi):  value = value * 10 + (char - '0')
;  for each digit, with an optional '-' in front.
; =============================================================

section .data
	digits  db  "0123456789ABCDEF"
	input   db  "-12345", 0
	t_atoi  db  "atoi(", 34, "-12345", 34, ") = "
	l_atoi  equ $ - t_atoi
	t_dbl   db  "times 2 = "
	l_dbl   equ $ - t_dbl
	t_hex   db  "in hex (two's complement) = "
	l_hex   equ $ - t_hex
	t_bin   db  "37 in binary = "
	l_bin   equ $ - t_bin
	nl      db  10

section .bss
	numbuf  resb 40             ; room for 32 binary digits and a sign

section .text
global _start

_start:
	mov		ecx, t_atoi
	mov		edx, l_atoi
	call	write_text
	mov		esi, input
	call	atoi                ; eax = -12345
	mov		edi, eax
	call	print_signed        ; -12345
	call	newline

	mov		ecx, t_dbl
	mov		edx, l_dbl
	call	write_text
	mov		eax, edi
	add		eax, eax            ; -24690
	mov		edi, eax
	call	print_signed
	call	newline

	mov		ecx, t_hex
	mov		edx, l_hex
	call	write_text
	mov		eax, edi
	mov		ebx, 16
	call	print_unsigned      ; FFFF9F8E
	call	newline

	mov		ecx, t_bin
	mov		edx, l_bin
	call	write_text
	mov		eax, 37
	mov		ebx, 2
	call	print_unsigned      ; 100101
	call	newline

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; atoi: the 0-terminated decimal text at esi -> eax.
atoi:
	xor		eax, eax            ; value = 0
	xor		edx, edx            ; edx = 1 if negative
	cmp		byte [esi], '-'
	jne		.digit
	mov		edx, 1
	inc		esi
.digit:
	movzx	ecx, byte [esi]
	cmp		ecx, '0'
	jb		.done               ; not a digit (the 0 at the end)
	cmp		ecx, '9'
	ja		.done
	imul	eax, 10             ; value = value * 10 + digit
	sub		ecx, '0'
	add		eax, ecx
	inc		esi
	jmp		.digit
.done:
	test	edx, edx
	jz		.positive
	neg		eax
.positive:
	ret

; print_signed: eax as a signed decimal number.
print_signed:
	test	eax, eax
	jns		.plus
	push	eax
	mov		byte [numbuf], '-'
	mov		ecx, numbuf
	mov		edx, 1
	call	write_text
	pop		eax
	neg		eax
.plus:
	mov		ebx, 10
	; (falls through into print_unsigned)

; print_unsigned: eax (unsigned) in base ebx (2 .. 16). Keeps esi, edi.
print_unsigned:
	push	esi
	push	edi
	lea		edi, [numbuf + 40]  ; fill from the end of the buffer
	mov		esi, ebx            ; the base
.next:
	xor		edx, edx            ; edx:eax / base
	div		esi                 ; eax = quotient, edx = the digit
	push	eax
	mov		al, dl
	mov		ebx, digits
	xlatb                       ; al = digits[digit]
	dec		edi
	mov		[edi], al
	pop		eax
	test	eax, eax
	jnz		.next               ; until nothing is left
	mov		ecx, edi            ; write(1, edi, end - edi)
	lea		edx, [numbuf + 40]
	sub		edx, edi
	call	write_text
	pop		edi
	pop		esi
	ret

; write_text: write(1, ecx, edx).
write_text:
	mov		eax, 4
	mov		ebx, 1
	int		0x80
	ret

newline:
	mov		ecx, nl
	mov		edx, 1
	call	write_text
	ret

; -------------------------------------------------------------
;  TRY IT
;   1. Step through print_unsigned and watch numbuf fill up from
;      the end in the Memory tab.
;   2. Print in base 8 or base 3.
;   3. atoi stops at the first non-digit: try "12abc".
;   4. What does atoi do with "99999999999" (too big)? Add an
;      overflow check (jo after imul and add).
; -------------------------------------------------------------
