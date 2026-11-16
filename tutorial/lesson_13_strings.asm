; =============================================================
;  Lesson 13: Strings
;  You will learn: strings are byte arrays ending in 0, how to
;  walk through them, and that characters are just numbers.
; =============================================================
;
;  A "C string" is a row of character codes with a 0 byte at the
;  end, so code can find where it stops:
;
;     word  db  "Hi!", 0     ->   72  105  33  0
;
;  Characters ARE numbers: 'A' = 65, 'a' = 97, '0' = 48. That
;  makes text processing simple arithmetic:
;     lower case -> upper case:  subtract 32  ('a' - 'A' = 32)
;     digit character -> value:  subtract '0'
;
;  We use byte-sized registers (AL, BL, ...) for characters,
;  and compare them with character constants:  cmp al, 'a' .
; =============================================================

section .data
	text    db  "Assembly is fun!", 0
	t_len   db  "length: ", 0
	t_up    db  "upper case: ", 0
	t_vow   db  "vowels: ", 0

section .bss
	upper   resb 64             ; room for the converted copy

section .text
global _start

_start:
	; ---- length: count bytes until the 0 ----
	xor		ecx, ecx            ; i = 0
.len:
	cmp		byte [text + ecx], 0
	je		.len_done
	inc		ecx
	jmp		.len
.len_done:
	mov		esi, ecx            ; 16
	mov		eax, 202
	mov		ebx, t_len
	int		0x80
	mov		eax, 200
	mov		ebx, esi
	int		0x80
	mov		eax, 205
	int		0x80

	; ---- copy to upper, converting a..z to A..Z ----
	xor		ecx, ecx
.copy:
	mov		al, [text + ecx]    ; al = text[i]
	cmp		al, 'a'
	jb		.store              ; below 'a': not a lower-case letter
	cmp		al, 'z'
	ja		.store              ; above 'z': neither
	sub		al, 32              ; 'a'..'z' -> 'A'..'Z'
.store:
	mov		[upper + ecx], al   ; upper[i] = al (the 0 is copied too)
	inc		ecx
	cmp		al, 0
	jne		.copy
	mov		eax, 202
	mov		ebx, t_up
	int		0x80
	mov		eax, 202
	mov		ebx, upper          ; ASSEMBLY IS FUN!
	int		0x80
	mov		eax, 205
	int		0x80

	; ---- count the vowels (upper case is enough now) ----
	xor		edi, edi            ; count = 0
	mov		esi, upper          ; esi = a POINTER to the current char
.vowel:
	mov		al, [esi]
	cmp		al, 0
	je		.vowel_done
	cmp		al, 'A'
	je		.is_vowel
	cmp		al, 'E'
	je		.is_vowel
	cmp		al, 'I'
	je		.is_vowel
	cmp		al, 'O'
	je		.is_vowel
	cmp		al, 'U'
	jne		.next
.is_vowel:
	inc		edi
.next:
	inc		esi                 ; move the pointer to the next char
	jmp		.vowel
.vowel_done:
	mov		eax, 202
	mov		ebx, t_vow
	int		0x80
	mov		eax, 200
	mov		ebx, edi            ; 4 (A, E, I, U)
	int		0x80
	mov		eax, 205
	int		0x80

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; -------------------------------------------------------------
;  TRY IT
;   1. Hover a character constant like 'a' in the code: the
;      editor shows its number in decimal, hex and binary.
;   2. Watch AL in the Registers panel while  .copy  runs.
;   3. Make a lower-case version (add 32 to 'A'..'Z').
;   4. Count the spaces, or reverse the string (hint: lesson 10).
;   5. The string instructions lodsb / stosb / movsb do the
;      "load / store and move the pointer" steps in one
;      instruction: see the Strings example (File > Examples).
;
;  Next: Lesson 14, reading input.
; -------------------------------------------------------------
