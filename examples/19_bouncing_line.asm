; =============================================================
;  Two points and a line, bouncing
;  Shows: Bresenham's line algorithm (a straight line with only
;  integer adds and compares, in every direction), arrays of
;  structures (two points: x, y, vx, vy), and exact bounces.
;  Each frame the old line is erased by drawing it again in the
;  background colour. Runs until stopped.
; =============================================================
;
;  Bresenham, from (x0, y0) to (x1, y1):
;     dx = |x1 - x0|,  sx = +1 or -1 (towards x1)
;     dy = -|y1 - y0|, sy = +1 or -1 (towards y1)
;     err = dx + dy
;     repeat: plot (x0, y0); stop at (x1, y1)
;             e2 = 2 * err
;             if e2 >= dy: err += dy, x0 += sx    (step in x)
;             if e2 <= dx: err += dx, y0 += sy    (step in y)
;  "err" keeps track of how far the pixels are from the true line.

MAXX    equ SCREEN_W - 1
MAXY    equ SCREEN_H - 1
BG      equ 17                  ; background
LINE    equ CYAN
POINT   equ WHITE

section .data
	; two points: x, y, vx, vy (16 bytes each)
	pts     dd  10, 20,  2,  1
			dd  100, 90, -1, 3
	colour  db  0

section .bss
	; the line being drawn (Bresenham's variables)
	lx      resd 1
	ly      resd 1
	ex      resd 1              ; end point
	ey      resd 1
	ldx     resd 1
	ldy     resd 1
	lsx     resd 1
	lsy     resd 1
	err     resd 1

section .text
global _start

_start:
	mov		eax, 210            ; clear_screen(BG)
	mov		ebx, BG
	int		0x80

.frame:
	mov		byte [colour], LINE
	call	draw_scene
	mov		eax, 213            ; wait_frame
	int		0x80
	mov		byte [colour], BG   ; erase: the same drawing in the background
	call	draw_scene
	call	move_points
	jmp		.frame

; draw_scene: the line, then the two points (3 x 3) on top, in [colour]
; (POINT for the points unless erasing).
draw_scene:
	mov		eax, [pts + 0]
	mov		[lx], eax
	mov		eax, [pts + 4]
	mov		[ly], eax
	mov		eax, [pts + 16]
	mov		[ex], eax
	mov		eax, [pts + 20]
	mov		[ey], eax
	call	line
	cmp		byte [colour], BG
	je		.erase_points
	mov		byte [colour], POINT
.erase_points:
	mov		ebx, [pts + 0]
	mov		ecx, [pts + 4]
	call	dot
	mov		ebx, [pts + 16]
	mov		ecx, [pts + 20]
	call	dot
	ret

; move_points: each point moves by its speed and reflects at the edges.
move_points:
	xor		esi, esi            ; offset of the point: 0, 16
.point:
	mov		eax, [pts + esi]    ; x
	add		eax, [pts + esi + 8]
	mov		ebx, MAXX
	lea		edi, [pts + esi + 8] ; &vx
	call	reflect
	mov		[pts + esi], eax
	mov		eax, [pts + esi + 4] ; y
	add		eax, [pts + esi + 12]
	mov		ebx, MAXY
	lea		edi, [pts + esi + 12] ; &vy
	call	reflect
	mov		[pts + esi + 4], eax
	add		esi, 16
	cmp		esi, 32
	jl		.point
	ret

; reflect: eax = a position that may be past 0 .. ebx; reflects it
; back inside and turns the speed at [edi] around.
reflect:
	cmp		eax, 0
	jge		.not_low
	neg		eax                 ; -p: as far inside as it was outside
	neg		dword [edi]
.not_low:
	cmp		eax, ebx
	jle		.done
	neg		eax
	lea		eax, [eax + ebx*2]  ; 2*max - p
	neg		dword [edi]
.done:
	ret

; dot: a 3 x 3 square in [colour] centred at (ebx, ecx).
dot:
	dec		ebx
	dec		ecx
	mov		eax, 212            ; fill_rect clips at the edges
	mov		edx, 3
	mov		esi, 3
	movzx	edi, byte [colour]
	int		0x80
	ret

; line: Bresenham from (lx, ly) to (ex, ey) in [colour].
line:
	mov		eax, [ex]           ; dx = |ex - lx|, sx = sign
	sub		eax, [lx]
	mov		dword [lsx], 1
	jge		.dx_pos
	neg		eax
	mov		dword [lsx], -1
.dx_pos:
	mov		[ldx], eax
	mov		eax, [ey]           ; dy = -|ey - ly|, sy = sign
	sub		eax, [ly]
	mov		dword [lsy], 1
	jge		.dy_pos
	neg		eax
	mov		dword [lsy], -1
.dy_pos:
	neg		eax
	mov		[ldy], eax
	add		eax, [ldx]          ; err = dx + dy
	mov		[err], eax
	mov		dl, [colour]
.plot:
	mov		eax, [ly]           ; pixel (lx, ly)
	shl		eax, 7
	add		eax, [lx]
	mov		[SCREEN + eax], dl
	mov		eax, [lx]           ; at the end point?
	cmp		eax, [ex]
	jne		.step
	mov		eax, [ly]
	cmp		eax, [ey]
	je		.done
.step:
	mov		eax, [err]
	add		eax, eax            ; e2 = 2 * err
	cmp		eax, [ldy]
	jl		.no_x               ; e2 >= dy: a step in x
	mov		ebx, [ldy]
	add		[err], ebx
	mov		ebx, [lsx]
	add		[lx], ebx
.no_x:
	cmp		eax, [ldx]
	jg		.plot               ; e2 <= dx: a step in y
	mov		ebx, [ldx]
	add		[err], ebx
	mov		ebx, [lsy]
	add		[ly], ebx
	jmp		.plot
.done:
	ret
