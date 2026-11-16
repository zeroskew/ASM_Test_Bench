; =============================================================
;  Keyboard: paint with the arrow keys
;  Click the Screen window, then hold the arrow keys to move the
;  brush. Space = random colour, C = clear the canvas.
;  key_down (215) tells whether a key is held right now;
;  read_key (214) returns key presses one at a time.
; =============================================================

section .data
	px      dd  62              ; brush position
	py      dd  62
	colour  dd  YELLOW
	BRUSH   equ 4
	BG      equ 235             ; canvas colour (dark grey)
	help    db  "Click the Screen window, then use the arrow keys.", 10
			db  "Space = new colour, C = clear.", 10, 0

section .text
global _start

_start:
	mov		eax, 202            ; print the help text
	mov		ebx, help
	int		0x80
	mov		eax, 210            ; clear_screen(BG)
	mov		ebx, BG
	int		0x80

.frame:
	; ---- move while arrow keys are held ----
	mov		eax, 215            ; key_down(KEY_LEFT)
	mov		ebx, KEY_LEFT
	int		0x80
	sub		[px], eax           ; eax is 1 while held, else 0
	mov		eax, 215
	mov		ebx, KEY_RIGHT
	int		0x80
	add		[px], eax
	mov		eax, 215
	mov		ebx, KEY_UP
	int		0x80
	sub		[py], eax
	mov		eax, 215
	mov		ebx, KEY_DOWN
	int		0x80
	add		[py], eax

	; ---- keep the brush on the screen ----
	mov		ebx, px
	call	clamp
	mov		ebx, py
	call	clamp

	; ---- handle key presses ----
.next_key:
	mov		eax, 214            ; read_key -> eax (0 = no more keys)
	int		0x80
	test	eax, eax
	jz		.paint
	cmp		eax, KEY_SPACE
	jne		.not_space
	mov		eax, 216            ; random(216) + 16: a colour from the cube
	mov		ebx, 216
	int		0x80
	add		eax, 16
	mov		[colour], eax
	jmp		.next_key
.not_space:
	cmp		eax, 'C'
	jne		.next_key
	mov		eax, 210            ; clear_screen(BG)
	mov		ebx, BG
	int		0x80
	jmp		.next_key

	; ---- paint at the brush position ----
.paint:
	mov		eax, 212            ; fill_rect(px, py, BRUSH, BRUSH, colour)
	mov		ebx, [px]
	mov		ecx, [py]
	mov		edx, BRUSH
	mov		esi, BRUSH
	mov		edi, [colour]
	int		0x80

	mov		eax, 213            ; wait_frame
	int		0x80
	jmp		.frame

; clamp(ebx = address of a coordinate): keep it in 0 .. 128-BRUSH
clamp:
	mov		eax, [ebx]
	cmp		eax, 0
	jge		.not_low
	mov		dword [ebx], 0
.not_low:
	cmp		eax, SCREEN_W - BRUSH
	jle		.done
	mov		dword [ebx], SCREEN_W - BRUSH
.done:
	ret
