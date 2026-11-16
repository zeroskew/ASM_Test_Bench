; =============================================================
;  Drawing on the screen
;  The screen is 128x128 pixels at address SCREEN (0xC000):
;  pixel (x, y) is the byte at SCREEN + y*128 + x, and the byte
;  is a palette colour (0-255). The Screen window opens by
;  itself when the program draws (or use View > Screen window).
; =============================================================

section .text
global _start

_start:
	; ---- background: clear_screen(17 = dark blue) ----
	mov		eax, 210
	mov		ebx, 17
	int		0x80

	; ---- the 16 basic colours as stripes along the top ----
	xor		ecx, ecx            ; x = 0..127
.stripe:
	mov		eax, ecx
	shr		eax, 3              ; colour = x / 8   (0..15)
	xor		edx, edx            ; y = 0..11
.stripe_px:
	mov		edi, edx
	shl		edi, 7              ; edi = y * 128
	add		edi, ecx            ;     + x
	mov		[SCREEN + edi], al  ; one byte = one pixel
	inc		edx
	cmp		edx, 12
	jl		.stripe_px
	inc		ecx
	cmp		ecx, SCREEN_W
	jl		.stripe

	; ---- a gradient from the 6x6x6 colour cube ----
	; colour = 16 + 36*red + 6*green + blue   (each 0..5)
	mov		esi, 12             ; divisor: 72 pixels / 6 levels
	mov		ecx, 20             ; y = 20..91
.grad_row:
	mov		ebx, 8              ; x = 8..79
.grad_px:
	mov		eax, ebx
	sub		eax, 8
	xor		edx, edx
	div		esi                 ; eax = red level
	imul	edi, eax, 36
	mov		eax, ecx
	sub		eax, 20
	xor		edx, edx
	div		esi                 ; eax = green level
	imul	eax, eax, 6
	add		edi, eax
	add		edi, 16 + 2         ; blue level 2
	mov		edx, ecx
	shl		edx, 7
	add		edx, ebx            ; edx = y*128 + x
	mov		eax, edi
	mov		[SCREEN + edx], al
	inc		ebx
	cmp		ebx, 80
	jl		.grad_px
	inc		ecx
	cmp		ecx, 92
	jl		.grad_row

	; ---- rectangles with fill_rect(x, y, width, height, colour) ----
	mov		eax, 212
	mov		ebx, 88
	mov		ecx, 24
	mov		edx, 32
	mov		esi, 32
	mov		edi, YELLOW
	int		0x80
	mov		eax, 212
	mov		ebx, 96
	mov		ecx, 32
	mov		edx, 16
	mov		esi, 16
	mov		edi, RED
	int		0x80

	; ---- a line: x = 0..127, y = 100 + x/5 ----
	xor		ecx, ecx
.line:
	mov		eax, ecx
	xor		edx, edx
	mov		ebx, 5
	div		ebx
	add		eax, 100            ; eax = y
	shl		eax, 7
	add		eax, ecx
	mov		byte [SCREEN + eax], WHITE
	inc		ecx
	cmp		ecx, SCREEN_W
	jl		.line

	mov		eax, 1              ; exit (the picture stays on the screen)
	xor		ebx, ebx
	int		0x80
