; =============================================================
;  Bouncing ball (animation)
;  Each frame: erase the ball, move it, bounce off the edges,
;  draw it again, then wait_frame (213) so it runs at ~60 frames
;  per second. Press F6 to pause and F10 to step a frame apart.
; =============================================================

section .data
	x       dd  20              ; ball position
	y       dd  50
	vx      dd  2               ; ball speed (pixels per frame)
	vy      dd  1
	SIZE    equ 6
	BG      equ 17              ; background colour (dark blue)

section .text
global _start

_start:
	mov		eax, 210            ; clear_screen(BG)
	mov		ebx, BG
	int		0x80

.frame:
	mov		edi, BG             ; erase the ball at its old position
	call	draw_ball

	mov		eax, [x]            ; x += vx, bounce at the left/right edges
	add		eax, [vx]
	mov		[x], eax
	cmp		eax, 0
	jle		.bounce_x
	cmp		eax, SCREEN_W - SIZE
	jl		.move_y
.bounce_x:
	neg		dword [vx]
.move_y:
	mov		eax, [y]            ; y += vy, bounce at the top/bottom
	add		eax, [vy]
	mov		[y], eax
	cmp		eax, 0
	jle		.bounce_y
	cmp		eax, SCREEN_H - SIZE
	jl		.draw
.bounce_y:
	neg		dword [vy]

.draw:
	mov		edi, YELLOW
	call	draw_ball

	mov		eax, 213            ; wait_frame
	int		0x80
	jmp		.frame

; draw_ball(edi = colour): a SIZE x SIZE square at (x, y)
draw_ball:
	mov		eax, 212            ; fill_rect(ebx=x, ecx=y, edx=w, esi=h, edi=colour)
	mov		ebx, [x]
	mov		ecx, [y]
	mov		edx, SIZE
	mov		esi, SIZE
	int		0x80
	ret
