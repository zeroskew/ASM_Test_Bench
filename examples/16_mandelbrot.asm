; =============================================================
;  Mandelbrot set
;  Shows: FIXED-POINT arithmetic (fractions with integers only),
;  imul + sar to multiply them, a nested pixel loop, and colours
;  by iteration count. Takes a moment: ~16000 pixels, up to 24
;  iterations each.
; =============================================================
;
;  The CPU here has no floating point, so 1.0 is stored as 4096
;  (12 fraction bits): 0.5 = 2048, -2.0 = -8192. Adding works as
;  usual; after a multiplication the result has 24 fraction bits,
;  so shift it back:  imul eax, ebx  /  sar eax, 12 .
;
;  For every pixel, c = (cx, cy) and z starts at 0; repeat
;     z = z*z + c      (zx' = zx² - zy² + cx,  zy' = 2·zx·zy + cy)
;  until |z|² > 4 (it escapes: colour by how fast) or MAXIT
;  times (it stays: the set itself, black).

ONE     equ 4096                ; 1.0 in fixed point
STEP    equ 96                  ; 3.0 / 128 pixels: the size of a pixel
MAXIT   equ 24

section .bss
	cx0     resd 1              ; c for the current pixel
	cy0     resd 1
	zx2     resd 1              ; zx² (kept while zy is computed)

section .text
global _start

_start:
	xor		edi, edi            ; py = 0 .. 127
.row:
	mov		eax, edi
	imul	eax, STEP
	sub		eax, 3*ONE/2        ; cy = -1.5 + py * step
	mov		[cy0], eax
	xor		esi, esi            ; px = 0 .. 127
.col:
	mov		eax, esi
	imul	eax, STEP
	sub		eax, 2*ONE          ; cx = -2.0 + px * step
	mov		[cx0], eax
	xor		ebx, ebx            ; zx = 0
	xor		ecx, ecx            ; zy = 0
	xor		ebp, ebp            ; iterations
.iterate:
	mov		eax, ebx
	imul	eax, ebx
	sar		eax, 12             ; zx²
	mov		edx, ecx
	imul	edx, ecx
	sar		edx, 12             ; zy²
	mov		[zx2], eax
	add		eax, edx
	cmp		eax, 4*ONE          ; |z|² > 4: escaped
	jg		.escaped
	mov		eax, ebx            ; zy = 2·zx·zy + cy
	imul	eax, ecx
	sar		eax, 11             ; (>> 12, then * 2)
	add		eax, [cy0]
	mov		ecx, eax
	mov		eax, [zx2]          ; zx = zx² - zy² + cx
	sub		eax, edx
	add		eax, [cx0]
	mov		ebx, eax
	inc		ebp
	cmp		ebp, MAXIT
	jl		.iterate
	mov		eax, BLACK          ; never escaped: inside the set
	jmp		.plot
.escaped:
	mov		eax, ebp            ; colour by speed: 16 + 6 * iterations
	imul	eax, 6
	add		eax, 16
.plot:
	mov		edx, edi            ; pixel (px, py) = SCREEN + py*128 + px
	shl		edx, 7
	add		edx, esi
	mov		[SCREEN + edx], al
	inc		esi
	cmp		esi, 128
	jl		.col
	inc		edi
	cmp		edi, 128
	jl		.row

	mov		eax, 1              ; exit(0): the picture stays on the Screen
	mov		ebx, 0
	int		0x80
