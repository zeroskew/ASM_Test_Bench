; =============================================================
;  Strings: length, uppercase, reverse
;  Shows: byte access (byte [..], al), lodsb/stosb, and that
;  characters are just numbers ('a' = 97, 'A' = 65).
; =============================================================

section .data
	text    db  "Assembly is fun!", 0

section .bss
	buffer  resb 64

section .text
global _start

_start:
	; ---- strlen(text) -> ecx ----
	mov		edi, text
	xor		ecx, ecx
.len:
	cmp		byte [edi + ecx], 0
	je		.len_done
	inc		ecx
	jmp		.len
.len_done:

	; ---- copy to buffer in uppercase ----
	mov		esi, text
	mov		edi, buffer
	cld                         ; lodsb/stosb move forwards
.upper:
	lodsb                       ; al = [esi], esi++
	cmp		al, 'a'
	jb		.store
	cmp		al, 'z'
	ja		.store
	sub		al, 32              ; 'a' - 'A' = 32
.store:
	stosb                       ; [edi] = al, edi++
	test	al, al
	jnz		.upper

	mov		eax, 202            ; print the uppercase copy
	mov		ebx, buffer
	int		0x80
	mov		eax, 205
	int		0x80

	; ---- reverse buffer in place (ecx = length) ----
	mov		esi, buffer                 ; left end
	lea		edi, [buffer + ecx - 1]     ; right end
.rev:
	cmp		esi, edi
	jae		.rev_done
	mov		al, [esi]
	mov		bl, [edi]
	mov		[esi], bl
	mov		[edi], al
	inc		esi
	dec		edi
	jmp		.rev
.rev_done:
	mov		eax, 202
	mov		ebx, buffer
	int		0x80
	mov		eax, 205
	int		0x80

	mov		eax, 1
	xor		ebx, ebx
	int		0x80
