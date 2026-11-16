; =============================================================
;  Advanced 3: Bit arrays (the sieve of Eratosthenes)
;  You will learn: storing one yes/no per BIT, the bit
;  instructions bt / bts, counting bits with popcnt, and finding
;  the highest bit with bsr.
; =============================================================
;
;  To find the primes below 100, cross out the multiples of every
;  number: whatever is left is prime. We need 100 yes/no values:
;  as bytes that's 100 bytes, as bits only 13. With a register as
;  the bit number, bt / bts work on a whole BIT ARRAY in memory:
;
;     bts [bits], ecx      ; set bit number ecx  (0 .. 127 here)
;     bt  [bits], ecx      ; CF = bit number ecx
;
;  (bit 37 is bit 5 of the dword at bits + 4: the CPU finds it.)
;
;     popcnt eax, src      ; eax = how many bits of src are 1
;     bsr    eax, src      ; eax = the number of the highest 1 bit
;                          ;       = floor(log2(src))
; =============================================================

section .data
	t_primes    db  "Primes below 100:", 10, 0
	t_count     db  "Count: ", 0
	t_log       db  "floor(log2(1000)) = ", 0

section .bss
	bits        resd 4          ; 128 bits: bit n = 1 means "n is not prime"

section .text
global _start

_start:
	bts		dword [bits], 0     ; 0 and 1 are not prime
	bts		dword [bits], 1

	; ---- the sieve ----
	mov		ecx, 2              ; i = 2
.outer:
	mov		eax, ecx
	imul	eax, ecx            ; i * i
	cmp		eax, 100
	jge		.print              ; done when i*i >= 100
	bt		[bits], ecx         ; i already crossed out?
	jc		.next
.cross:
	bts		[bits], eax         ; cross out i*i, i*i+i, ...
	add		eax, ecx
	cmp		eax, 100
	jl		.cross
.next:
	inc		ecx
	jmp		.outer

	; ---- print the numbers whose bit is still 0 ----
.print:
	mov		eax, 202
	mov		ebx, t_primes
	int		0x80
	xor		ecx, ecx
.each:
	bt		[bits], ecx
	jc		.skip
	mov		eax, 200
	mov		ebx, ecx
	int		0x80
	mov		eax, 201
	mov		ebx, ' '
	int		0x80
.skip:
	inc		ecx
	cmp		ecx, 100
	jl		.each
	mov		eax, 205
	int		0x80

	; ---- count: 100 minus the number of 1 bits ----
	xor		esi, esi
	xor		ecx, ecx
.count:
	popcnt	eax, [bits + ecx*4]
	add		esi, eax            ; esi = numbers crossed out (75)
	inc		ecx
	cmp		ecx, 4
	jl		.count
	mov		eax, 202
	mov		ebx, t_count
	int		0x80
	mov		eax, 200
	mov		ebx, 100
	sub		ebx, esi            ; 25 primes
	int		0x80
	mov		eax, 205
	int		0x80

	; ---- bsr: the highest bit of 1000 = floor(log2(1000)) ----
	mov		eax, 1000
	bsr		esi, eax            ; 1000 = 0b1111101000 -> 9
	mov		eax, 202
	mov		ebx, t_log
	int		0x80
	mov		eax, 200
	mov		ebx, esi
	int		0x80
	mov		eax, 205
	int		0x80

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; -------------------------------------------------------------
;  TRY IT
;   1. Look at "bits" in the Memory tab after the sieve: every 1
;      is a crossed-out number. Switch the Registers panel to bin.
;   2. Go up to 1000: resd 32 (1024 bits), and 1000 in the loops.
;      There are 168 primes below 1000.
;   3. Use bsf on a dword of the array to find the first number
;      that is NOT crossed out (hint: not the dword first).
; -------------------------------------------------------------
