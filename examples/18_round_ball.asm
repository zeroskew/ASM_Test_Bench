; =============================================================
;  Round ball bouncing off the edges
;  Shows: drawing a filled circle pixel by pixel (x² + y² ≤ r²),
;  exact bounces (the ball is reflected at the edge, so it always
;  touches it and never leaves the screen), and erasing by
;  drawing again in the background colour. Runs until stopped.
; =============================================================
;
;  Moving: x = x + vx. If that goes past the left edge (x < R),
;  the ball would be "inside" the wall by R - x pixels; reflected,
;  it is that far out again:  x = R + (R - x) = 2R - x,  and the
;  speed turns around:  vx = -vx.  Same for the other edges.

R       equ 6                   ; radius
LEFT    equ R                   ; the centre stays between these
RIGHT   equ SCREEN_W - 1 - R
TOP     equ R
BOTTOM  equ SCREEN_H - 1 - R
BG      equ 17                  ; background (dark blue)
BALL    equ 214                 ; orange

section .data
	x       dd  30              ; centre
	y       dd  40
	vx      dd  3               ; speed in pixels per frame
	vy      dd  2

section .text
global _start

_start:
	mov		eax, 210            ; clear_screen(BG)
	mov		ebx, BG
	int		0x80

.frame:
	mov		ebx, BALL
	call	draw_ball
	mov		eax, 213            ; wait_frame
	int		0x80
	mov		ebx, BG             ; erase it where it was
	call	draw_ball

	; ---- x: move, reflect at the left / right edge ----
	mov		eax, [x]
	add		eax, [vx]
	cmp		eax, LEFT
	jge		.not_left
	neg		eax
	add		eax, 2*LEFT         ; x = 2*LEFT - x
	neg		dword [vx]
.not_left:
	cmp		eax, RIGHT
	jle		.x_done
	neg		eax
	add		eax, 2*RIGHT        ; x = 2*RIGHT - x
	neg		dword [vx]
.x_done:
	mov		[x], eax

	; ---- y: the same at the top / bottom ----
	mov		eax, [y]
	add		eax, [vy]
	cmp		eax, TOP
	jge		.not_top
	neg		eax
	add		eax, 2*TOP
	neg		dword [vy]
.not_top:
	cmp		eax, BOTTOM
	jle		.y_done
	neg		eax
	add		eax, 2*BOTTOM
	neg		dword [vy]
.y_done:
	mov		[y], eax
	jmp		.frame

; draw_ball(bl = colour): a filled circle of radius R at (x, y).
; For every (dx, dy) in the square around the centre, plot the pixel
; if dx² + dy² <= R².
draw_ball:
	mov		ecx, -R             ; dy
.row:
	mov		edx, -R             ; dx
.col:
	mov		eax, edx
	imul	eax, edx            ; dx²
	mov		esi, ecx
	imul	esi, ecx            ; dy²
	add		eax, esi
	cmp		eax, R*R
	jg		.outside
	mov		edi, [y]            ; address = SCREEN + (y+dy)*128 + (x+dx)
	add		edi, ecx
	shl		edi, 7
	add		edi, [x]
	add		edi, edx
	mov		[SCREEN + edi], bl
.outside:
	inc		edx
	cmp		edx, R
	jle		.col
	inc		ecx
	cmp		ecx, R
	jle		.row
	ret
