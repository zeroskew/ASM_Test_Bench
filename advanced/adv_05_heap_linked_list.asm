; =============================================================
;  Advanced 5: The heap and a linked list
;  You will learn: getting memory while the program runs (brk),
;  writing a tiny allocator, structures with fields at fixed
;  offsets, and building / walking / reversing a linked list.
; =============================================================
;
;  .data and .bss have a fixed size. Memory you need while the
;  program runs comes from the HEAP: the space right after .bss.
;  The brk system call (EAX = 45) moves its end:
;     brk(0)        -> eax = where the heap ends now
;     brk(address)  -> moves the end; eax = the new end
;  Our allocator just moves the end up by the size we need (a
;  "bump allocator": it never gives memory back).
;
;  A list NODE is 8 bytes:
;     [node + 0]  value      (dd)
;     [node + 4]  next       (the address of the next node, 0 = end)
;  so the list is a chain of addresses: head -> node -> node -> 0.
; =============================================================

VALUE   equ 0                   ; field offsets inside a node
NEXT    equ 4
NODE    equ 8                   ; the size of a node

section .data
	t_list  db  "list:     ", 0
	t_rev   db  "reversed: ", 0
	t_used  db  "heap used: ", 0
	t_bytes db  " bytes", 10, 0

section .bss
	heap0   resd 1              ; where the heap started

section .text
global _start

_start:
	mov		eax, 45             ; brk(0): where does the heap start?
	xor		ebx, ebx
	int		0x80
	mov		[heap0], eax

	; ---- build the list: push 1, 4, 9, 16, 25 at the front ----
	xor		esi, esi            ; head = 0 (empty list)
	mov		edi, 1              ; i
.build:
	mov		ecx, NODE
	call	alloc               ; eax = a new node
	mov		edx, edi
	imul	edx, edi            ; i * i
	mov		[eax + VALUE], edx
	mov		[eax + NEXT], esi   ; new node -> old head
	mov		esi, eax            ; head = new node
	inc		edi
	cmp		edi, 5
	jle		.build

	mov		ebx, t_list
	call	print_list          ; 25 16 9 4 1 (the last one added is first)

	; ---- reverse it: turn every "next" arrow around ----
	xor		ebx, ebx            ; prev = 0
.reverse:
	test	esi, esi
	jz		.reversed
	mov		edx, [esi + NEXT]   ; keep the rest of the list
	mov		[esi + NEXT], ebx   ; this node -> prev
	mov		ebx, esi            ; prev = this node
	mov		esi, edx            ; go on with the rest
	jmp		.reverse
.reversed:
	mov		esi, ebx            ; the last node is the new head
	mov		ebx, t_rev
	call	print_list          ; 1 4 9 16 25

	; ---- how much heap did we use? ----
	mov		eax, 45
	xor		ebx, ebx
	int		0x80
	sub		eax, [heap0]        ; 5 nodes * 8 bytes = 40
	mov		esi, eax
	mov		eax, 202
	mov		ebx, t_used
	int		0x80
	mov		eax, 200
	mov		ebx, esi
	int		0x80
	mov		eax, 202
	mov		ebx, t_bytes
	int		0x80

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; alloc: ecx = size in bytes -> eax = the address of a new block.
; (Exits with code 2 if the heap is full.)
alloc:
	push	ebx
	push	edx
	mov		eax, 45
	xor		ebx, ebx
	int		0x80                ; eax = the current end = our block
	mov		edx, eax
	lea		ebx, [eax + ecx]    ; the new end
	mov		eax, 45
	int		0x80
	cmp		eax, ebx            ; did the end move?
	jne		.full
	mov		eax, edx
	pop		edx
	pop		ebx
	ret
.full:
	mov		eax, 1
	mov		ebx, 2
	int		0x80

; print_list: prints the text at ebx, then the values of the
; list starting at esi.
print_list:
	push	esi
	mov		eax, 202
	int		0x80
.next:
	test	esi, esi            ; 0 = the end of the list
	jz		.end
	mov		eax, 200
	mov		ebx, [esi + VALUE]
	int		0x80
	mov		eax, 201
	mov		ebx, ' '
	int		0x80
	mov		esi, [esi + NEXT]   ; follow the arrow
	jmp		.next
.end:
	mov		eax, 205
	int		0x80
	pop		esi
	ret

; -------------------------------------------------------------
;  TRY IT
;   1. Look at the Memory tab right after .bss while the list is
;      built: each node is 8 bytes, value then the next address.
;   2. Hover ESI while walking the list: it holds an address.
;   3. Add a "sum" function that walks the list (55).
;   4. Insert 10 between 9 and 16: allocate a node, then change
;      two "next" fields.
;   5. Ask for a huge block (mov ecx, 100000): the heap can't grow
;      into the screen memory, and alloc exits with code 2.
; -------------------------------------------------------------
