; =============================================================
;  Advanced 4: String instructions (strlen, memcpy, strcmp)
;  You will learn: the string instructions with rep prefixes,
;  which do a whole loop in one instruction, and the C library
;  functions built from them.
; =============================================================
;
;  The string instructions work on [ESI] (source) and [EDI]
;  (destination) and move the pointers on by themselves
;  (forwards after cld):
;
;     movsb      [edi] = [esi], both + 1          (copy a byte)
;     stosb      [edi] = al, edi + 1             (fill)
;     scasb      compare al with [edi], edi + 1  (search)
;     cmpsb      compare [esi] with [edi]        (compare)
;
;  A rep prefix repeats them ECX times:
;     rep   movsb     copy ECX bytes
;     rep   stosb     fill ECX bytes with AL
;     repne scasb     search until [edi] == al  (or ECX runs out)
;     repe  cmpsb     compare while the bytes are equal
; =============================================================

section .data
	s1      db  "assembly", 0
	s2      db  "assemble", 0
	t_len   db  "strlen(s1) = ", 0
	t_copy  db  "copy: ", 0
	t_cmp   db  "strcmp(s1, s2) = ", 0
	t_at    db  " (first difference at index ", 0
	t_close db  ")", 10, 0

section .bss
	copy    resb 32
	line    resb 21

section .text
global _start

_start:
	cld                         ; string instructions go forwards

	; ---- strlen ----
	mov		edi, s1
	call	strlen              ; eax = 8
	mov		esi, eax
	mov		eax, 202
	mov		ebx, t_len
	int		0x80
	mov		eax, 200
	mov		ebx, esi
	int		0x80
	mov		eax, 205
	int		0x80

	; ---- memcpy(copy, s1, strlen + 1) ----
	mov		ecx, esi
	inc		ecx                 ; + the 0 at the end
	mov		esi, s1
	mov		edi, copy
	rep     movsb               ; copies ecx bytes
	mov		eax, 202
	mov		ebx, t_copy
	int		0x80
	mov		eax, 202
	mov		ebx, copy
	int		0x80
	mov		eax, 205
	int		0x80

	; ---- strcmp ----
	mov		esi, s1
	mov		edi, s2
	call	strcmp              ; eax = 'y' - 'e' = 20, ecx = index
	mov		esi, eax
	mov		edi, ecx
	mov		eax, 202
	mov		ebx, t_cmp
	int		0x80
	mov		eax, 200
	mov		ebx, esi
	int		0x80
	mov		eax, 202
	mov		ebx, t_at
	int		0x80
	mov		eax, 200
	mov		ebx, edi
	int		0x80
	mov		eax, 202
	mov		ebx, t_close
	int		0x80

	; ---- fill: 20 '=' and a 0 ----
	mov		edi, line
	mov		al, '='
	mov		ecx, 20
	rep     stosb
	mov		byte [edi], 0       ; edi already points after the 20
	mov		eax, 202
	mov		ebx, line
	int		0x80
	mov		eax, 205
	int		0x80

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; strlen: eax = the length of the 0-terminated string at edi.
strlen:
	push	edi
	mov		ecx, -1             ; "no limit"
	xor		al, al              ; search for the 0
	repne   scasb               ; edi stops just after the 0
	mov		eax, -2
	sub		eax, ecx            ; length = -2 - ecx
	pop		edi
	ret

; strcmp: compares the strings at esi and edi.
;   eax = 0 if equal, else (first different byte of esi) - (of edi);
;   ecx = the index of that byte.
strcmp:
	push	esi
	push	edi
	call	strlen              ; length of the edi string (+1 for the 0)
	lea		ecx, [eax + 1]
	mov		edx, esi            ; remember the start
	repe    cmpsb               ; stops after the first difference
	je		.equal              ; ZF = 1: no difference at all
	movzx	eax, byte [esi - 1]
	movzx	ecx, byte [edi - 1]
	sub		eax, ecx
	mov		ecx, esi
	sub		ecx, edx
	dec		ecx                 ; index of the different byte
	pop		edi
	pop		esi
	ret
.equal:
	xor		eax, eax
	pop		edi
	pop		esi
	ret

; -------------------------------------------------------------
;  TRY IT
;   1. Step over  rep movsb  with F10: one step copies 9 bytes.
;      Watch ESI, EDI and ECX change. Then try F11 on it.
;   2. Why  -2 - ecx  in strlen? ECX starts at -1 and goes down
;      once per byte, including the 0.
;   3. Compare "assembly" with itself: strcmp returns 0.
;   4. Write strchr: the address of the first 'm' in s1
;      (repne scasb with al = 'm').
; -------------------------------------------------------------
