; =============================================================
;  Lesson 16: Drawing on the screen
;  You will learn: memory-mapped graphics: the screen is just
;  memory, and every byte is one pixel.
; =============================================================
;
;  The Screen window (View menu; it opens by itself when a
;  program draws) shows 128 x 128 pixels. They live in memory
;  starting at the address SCREEN, one byte per pixel, row
;  after row:
;
;     pixel (x, y)  is the byte at  SCREEN + y*SCREEN_W + x
;
;  The byte is a colour number from a 256-colour palette:
;  0-15 are basic colours (built-in names BLACK, RED, GREEN,
;  YELLOW, BLUE, MAGENTA, CYAN, WHITE, GRAY), 16-231 a mix of
;  red/green/blue, 232-255 greys.
;
;  So drawing is nothing but mov into memory, with everything
;  from the earlier lessons: loops, arrays, addresses.
;  (There are also helper calls: 210 clear_screen, 211
;  set_pixel, 212 fill_rect, see the Reference tab.)
; =============================================================

section .text
global _start

_start:
	; ---- one pixel: (64, 10) in white ----
	mov		byte [SCREEN + 10*SCREEN_W + 64], WHITE

	; ---- a horizontal line: y = 20, x = 10..117 ----
	mov		ecx, 10                     ; x
.hline:
	mov		byte [SCREEN + 20*SCREEN_W + ecx], YELLOW
	inc		ecx
	cmp		ecx, 117
	jle		.hline

	; ---- a filled square: x 30..59, y 40..69 ----
	mov		edx, 40                     ; y
.row:
	mov		edi, edx
	imul	edi, SCREEN_W               ; edi = y * SCREEN_W
	add		edi, SCREEN                 ; edi = address of (0, y)
	mov		ecx, 30                     ; x
.col:
	mov		byte [edi + ecx], RED
	inc		ecx
	cmp		ecx, 59
	jle		.col
	inc		edx
	cmp		edx, 69
	jle		.row

	; ---- a diagonal line with a colour gradient ----
	xor		ecx, ecx                    ; i = 0
.diag:
	mov		edi, ecx
	add		edi, 40                     ; y = 40 + i
	imul	edi, SCREEN_W
	add		edi, SCREEN
	mov		eax, ecx
	add		eax, 70                     ; x = 70 + i
	mov		ebx, ecx
	add		ebx, 232                    ; colour: the greys 232..255
	mov		[edi + eax], bl
	inc		ecx
	cmp		ecx, 23
	jle		.diag

	mov		eax, 1                      ; exit(0)
	mov		ebx, 0
	int		0x80

; -------------------------------------------------------------
;  TRY IT
;   1. Step through the first lines with F10 and watch the
;      Screen window: one pixel at a time. Step Back undoes
;      pixels too.
;   2. Draw a vertical line (x stays the same, y changes).
;   3. Make the square blue, or draw a border instead of a
;      filled square.
;   4. Look at the examples "Drawing", "Bouncing ball" and
;      "Keyboard" (File > Examples) for animation and keys.
;
;  You finished the tutorial! The Reference tab (bottom panel)
;  lists every instruction and system call.
; -------------------------------------------------------------
