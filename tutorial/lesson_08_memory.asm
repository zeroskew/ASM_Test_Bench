; =============================================================
;  Lesson 8: Variables in memory
;  You will learn: the difference between an address and the
;  value stored there, [ ] brackets, operand sizes (byte, word,
;  dword), and the .bss section.
; =============================================================
;
;  Registers are few. Everything else lives in MEMORY: a long
;  row of bytes, each with a number (its ADDRESS).
;
;     count       the ADDRESS of the variable (a number like 0x4000)
;     [count]     the VALUE stored at that address
;
;  So  mov eax, count  puts the address into eax, while
;      mov eax, [count]  reads the value. This is the most
;  important idea in this lesson.
;
;  Sizes: dd = 4 bytes (dword), dw = 2 bytes (word), db = 1 byte.
;  When the size isn't clear from a register, say it:
;     mov   dword [count], 5     ; write 4 bytes
;     inc   byte [small]         ; change 1 byte
;
;  The .bss section reserves space without giving it a value
;  (it starts as zeros):  resd 1 = room for one dword.
;  At most ONE operand of an instruction can be memory:
;  mov [a], [b] doesn't exist; go through a register.
; =============================================================

section .data
	count   dd  10              ; a 32-bit variable, starts at 10
	small   db  250             ; an 8-bit variable
	t_count db  "count = ", 0
	t_small db  "small = ", 0
	t_copy  db  "copy  = ", 0
	t_addr  db  "the address of count = ", 0

section .bss
	copy    resd 1              ; one dword, starts as 0

section .text
global _start

_start:
	mov		eax, [count]        ; eax = 10   (read the value)
	add		eax, 5
	mov		[count], eax        ; count = 15 (write it back)
	add		dword [count], 100  ; count = 115, done in memory

	inc		byte [small]        ; 251
	add		byte [small], 10    ; 261 doesn't fit in a byte: 5

	mov		eax, [count]        ; copy = count (memory to memory
	mov		[copy], eax         ; goes through a register)

	; ---- print them ----
	mov		ebx, t_count
	mov		esi, [count]
	call	show
	mov		ebx, t_small
	movzx	esi, byte [small]   ; read 1 byte, zero-extend it to 32 bits
	call	show
	mov		ebx, t_copy
	mov		esi, [copy]
	call	show
	mov		eax, 202
	mov		ebx, t_addr
	int		0x80
	mov		eax, 204            ; print_hex: an address, not a value
	mov		ebx, count          ; no brackets!
	int		0x80
	mov		eax, 205
	int		0x80

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; show: prints the text at ebx, then the number in esi.
show:
	mov		eax, 202
	int		0x80
	mov		eax, 200
	mov		ebx, esi
	int		0x80
	mov		eax, 205
	int		0x80
	ret

; -------------------------------------------------------------
;  TRY IT
;   1. Open the Memory tab while stepping: the bytes that just
;      changed are highlighted. The dword 115 is stored as
;      73 00 00 00: x86 keeps the LOWEST byte first
;      ("little endian").
;   2. Remove the brackets in  mov eax, [count]  and run: eax
;      now gets the address (0x4000) instead of the value.
;   3. Write  add [count], 100  without "dword". The assembler
;      asks you for the size.
;
;  Next: Lesson 9, arrays.
; -------------------------------------------------------------
