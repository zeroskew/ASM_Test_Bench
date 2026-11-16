; =============================================================
;  Advanced 2: Numbers bigger than 32 bits
;  You will learn: 64-bit add / subtract with the carry (adc,
;  sbb), the 64-bit result of mul, 64-bit shifts (shld), and
;  printing a 64-bit number in hex with a lookup table (xlat).
; =============================================================
;
;  A 64-bit number is kept in two registers, written EDX:EAX
;  (EDX = the upper 32 bits). The CPU only adds 32 bits at a
;  time, so:
;
;     add  eax, low        ; low halves; a carry goes into CF
;     adc  edx, high       ; high halves + CF ("add with carry")
;
;  Subtract the same way with sub / sbb ("with borrow").
;  mul ecx  multiplies EAX by ECX and gives the full 64-bit
;  product in EDX:EAX. To shift EDX:EAX left by 4:
;
;     shld edx, eax, 4     ; EDX gets the 4 top bits of EAX
;     shl  eax, 4
;
;  To print, we take 4 bits (one hex digit) at a time from the
;  top and look the character up in a table with xlat:
;  AL = [EBX + AL].
; =============================================================

section .data
	digits  db  "0123456789ABCDEF"
	t_add   db  "0x1FFFFFFFF + 1     = 0x", 0
	t_mul   db  "100000 * 300000     = 0x", 0
	t_shl   db  "that << 4           = 0x", 0
	t_sub   db  "that - 1            = 0x", 0

section .text
global _start

_start:
	; ---- 0x1_FFFFFFFF + 1: the carry moves into the upper half ----
	mov		eax, 0xFFFFFFFF     ; low half
	mov		edx, 1              ; high half
	add		eax, 1              ; eax = 0, CF = 1
	adc		edx, 0              ; edx = 1 + 0 + CF = 2
	mov		ebx, t_add
	call	show64              ; 0000000200000000

	; ---- a 64-bit product ----
	mov		eax, 100000
	mov		ecx, 300000
	mul		ecx                 ; edx:eax = 30,000,000,000
	mov		ebx, t_mul
	call	show64              ; 00000006FC23AC00

	; ---- shift the whole 64-bit number left by 4 ----
	shld	edx, eax, 4
	shl		eax, 4
	mov		ebx, t_shl
	call	show64              ; 0000006FC23AC000

	; ---- subtract 1: the borrow goes into the upper half ----
	sub		eax, 1
	sbb		edx, 0
	mov		ebx, t_sub
	call	show64              ; 0000006FC23ABFFF

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; show64: prints the text at ebx, then edx:eax as 16 hex digits
; (keeps edx:eax: the next step goes on with it).
show64:
	push	eax
	mov		eax, 202
	int		0x80
	pop		eax
	call	print_hex64
	push	eax
	mov		eax, 205
	int		0x80
	pop		eax
	ret

; print_hex64: edx:eax as 16 hex digits (keeps edx:eax).
print_hex64:
	push	esi
	push	edi
	push	eax
	push	edx
	mov		esi, eax            ; edi:esi = the number (a copy we shift)
	mov		edi, edx
	mov		ecx, 16             ; 16 digits
.digit:
	mov		eax, edi
	shr		eax, 28             ; the top 4 bits = one hex digit
	mov		ebx, digits
	xlatb                       ; al = digits[al]
	mov		ebx, eax
	mov		eax, 201            ; print_char (system calls keep ecx)
	int		0x80
	shld	edi, esi, 4         ; shift edi:esi left by 4
	shl		esi, 4
	loop	.digit
	pop		edx
	pop		eax
	pop		edi
	pop		esi
	ret

; -------------------------------------------------------------
;  TRY IT
;   1. Step over  add eax, 1  and look at CF in the Flags panel,
;      then see adc use it.
;   2. Replace  adc edx, 0  with  add edx, 0 : the carry is lost
;      and the result is wrong by 2^32.
;   3. Multiply two 64-bit numbers? It takes 3 or 4 muls of the
;      32-bit halves (like long multiplication on paper).
;   4. Write print_dec64 (divide by 10 twice per digit: first the
;      high half, then the remainder:low half).
; -------------------------------------------------------------
