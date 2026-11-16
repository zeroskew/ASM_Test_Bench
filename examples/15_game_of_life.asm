; =============================================================
;  Game of Life
;  Shows: a 2D grid in memory, counting neighbours with
;  wrap-around (and with SIZE-1), double buffering (compute the
;  next generation into a second grid, then copy it with
;  rep movsb), and drawing with fill_rect. Runs until you stop it.
; =============================================================
;
;  Rules, for every cell of a 32 x 32 grid:
;    - a live cell with 2 or 3 live neighbours stays alive,
;    - a dead cell with exactly 3 live neighbours comes alive,
;    - everything else dies / stays dead.
;  A glider walks diagonally across the screen, a blinker
;  blinks, and an R-pentomino grows into a small explosion.

SIZE    equ 32                  ; cells per row (a power of two)
CELL    equ 4                   ; pixels per cell: 32 * 4 = 128
DEAD    equ 17                  ; colours
ALIVE   equ YELLOW

section .bss
	cells   resb SIZE*SIZE      ; 1 = alive
	next    resb SIZE*SIZE      ; the next generation

section .text
global _start

_start:
	; ---- a glider (top left) ----
	mov		byte [cells + 0*SIZE + 1], 1
	mov		byte [cells + 1*SIZE + 2], 1
	mov		byte [cells + 2*SIZE + 0], 1
	mov		byte [cells + 2*SIZE + 1], 1
	mov		byte [cells + 2*SIZE + 2], 1
	; ---- a blinker ----
	mov		byte [cells + 10*SIZE + 10], 1
	mov		byte [cells + 10*SIZE + 11], 1
	mov		byte [cells + 10*SIZE + 12], 1
	; ---- an R-pentomino ----
	mov		byte [cells + 20*SIZE + 21], 1
	mov		byte [cells + 20*SIZE + 22], 1
	mov		byte [cells + 21*SIZE + 20], 1
	mov		byte [cells + 21*SIZE + 21], 1
	mov		byte [cells + 22*SIZE + 21], 1

.frame:
	call	draw
	mov		eax, 213            ; wait_frame: one generation per frame
	int		0x80
	call	generation
	jmp		.frame

; draw: every cell as a CELL x CELL square.
draw:
	xor		ebp, ebp            ; i = 0 .. SIZE*SIZE-1
.cell:
	mov		ebx, ebp
	and		ebx, SIZE-1         ; x = i % SIZE
	shl		ebx, 2              ; * CELL
	mov		ecx, ebp
	shr		ecx, 5              ; y = i / SIZE
	shl		ecx, 2
	mov		edx, CELL           ; width
	mov		esi, CELL           ; height
	mov		edi, DEAD
	cmp		byte [cells + ebp], 0
	je		.paint
	mov		edi, ALIVE
.paint:
	mov		eax, 212            ; fill_rect(x, y, w, h, colour)
	int		0x80
	inc		ebp
	cmp		ebp, SIZE*SIZE
	jl		.cell
	ret

; generation: cells -> next by the rules, then copy next -> cells.
generation:
	xor		ebp, ebp            ; i
.cell:
	xor		eax, eax            ; live neighbours
	mov		ecx, -1             ; dy = -1 .. 1
.dy:
	mov		edx, -1             ; dx = -1 .. 1
.dx:
	mov		esi, ecx
	or		esi, edx
	jz		.skip               ; dx = dy = 0: the cell itself
	mov		esi, ebp
	shr		esi, 5
	add		esi, ecx
	and		esi, SIZE-1         ; (y + dy), wrapped around
	shl		esi, 5
	mov		edi, ebp
	and		edi, SIZE-1
	add		edi, edx
	and		edi, SIZE-1         ; (x + dx), wrapped around
	add		esi, edi
	movzx	edi, byte [cells + esi]
	add		eax, edi
.skip:
	inc		edx
	cmp		edx, 1
	jle		.dx
	inc		ecx
	cmp		ecx, 1
	jle		.dy
	; alive next: 3 neighbours, or 2 and alive now
	xor		ebx, ebx
	cmp		eax, 3
	je		.born
	cmp		eax, 2
	jne		.store
	movzx	ebx, byte [cells + ebp]
	jmp		.store
.born:
	mov		ebx, 1
.store:
	mov		[next + ebp], bl
	inc		ebp
	cmp		ebp, SIZE*SIZE
	jl		.cell
	; copy the new generation over the old one
	mov		esi, next
	mov		edi, cells
	mov		ecx, SIZE*SIZE
	cld
	rep     movsb
	ret
