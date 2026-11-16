; =============================================================
;  Advanced 8: Jump tables and function pointers
;  You will learn: addresses of code stored in data, jumping /
;  calling through memory (jmp [table + i*4], call [ops + i*4]),
;  and how a "switch" and "virtual functions" work underneath.
; =============================================================
;
;  A label in the code is just an address, so it can be stored
;  in data like any number:
;
;     ops   dd  op_add, op_sub, op_mul, op_div
;
;  Then  call [ops + ecx*4]  calls the function whose address is
;  entry ecx of the table: no chain of cmp / je needed. This is
;  how C compiles a big switch, and how C++ calls virtual
;  functions.
;
;  Always check the index first: an index outside the table
;  jumps to whatever number happens to be there.
; =============================================================

section .data
	ops     dd  op_add, op_sub, op_mul, op_div      ; function pointers
	names   db  "+-*/"                              ; their symbols
	nops    equ 4
	a       dd  20
	b       dd  5

	sizes   dd  case_small, case_medium, case_large ; a "switch"
	t_small db  "small", 10, 0
	t_med   db  "medium", 10, 0
	t_large db  "large", 10, 0
	t_bad   db  "out of range", 10, 0

section .text
global _start

_start:
	; ---- call every operation through the table ----
	xor		esi, esi            ; i = 0
.each:
	mov		eax, 200            ; print "20 + 5 = "
	mov		ebx, [a]
	int		0x80
	mov		eax, 201
	mov		ebx, ' '
	int		0x80
	movzx	ebx, byte [names + esi]
	int		0x80
	mov		ebx, ' '
	int		0x80
	mov		eax, 200
	mov		ebx, [b]
	int		0x80
	mov		eax, 201
	mov		ebx, ' '
	int		0x80
	mov		ebx, '='
	int		0x80
	mov		ebx, ' '
	int		0x80

	mov		eax, [a]            ; arguments: eax, ebx
	mov		ebx, [b]
	call	[ops + esi*4]       ; eax = result
	mov		ebx, eax
	mov		eax, 200
	int		0x80
	mov		eax, 205
	int		0x80
	inc		esi
	cmp		esi, nops
	jl		.each

	; ---- a switch on 0, 1, 2 (and an out-of-range value) ----
	mov		eax, 1
	call	switch_size         ; medium
	mov		eax, 2
	call	switch_size         ; large
	mov		eax, 7
	call	switch_size         ; out of range

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; The operations: eax = eax (op) ebx.
op_add:
	add		eax, ebx
	ret
op_sub:
	sub		eax, ebx
	ret
op_mul:
	imul	eax, ebx
	ret
op_div:
	cdq
	idiv	ebx
	ret

; switch (eax) { case 0: small  case 1: medium  case 2: large }
switch_size:
	cmp		eax, 2
	ja		.bad                ; unsigned: also catches negatives
	jmp		[sizes + eax*4]     ; jump to case eax
.bad:
	mov		ebx, t_bad
	jmp		print_it
case_small:
	mov		ebx, t_small
	jmp		print_it
case_medium:
	mov		ebx, t_med
	jmp		print_it
case_large:
	mov		ebx, t_large
print_it:
	mov		eax, 202
	int		0x80
	ret

; -------------------------------------------------------------
;  TRY IT
;   1. Look at "ops" in the Memory tab: four code addresses
;      (0x10xx). Hover a label like op_add to compare.
;   2. Add a fifth operation (op_mod: the remainder) to both
;      tables and set nops to 5.
;   3. Remove the  ja .bad  check and call switch_size with 7.
;      jmp jumps to a "random" address and the program stops.
;   4. Step INTO  call [ops + esi*4]  (F11): which function runs?
; -------------------------------------------------------------
