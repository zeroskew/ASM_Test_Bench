; =============================================================
;  Lesson 12: Stack frames
;  You will learn: passing arguments on the stack (the way C
;  does it), EBP as a frame pointer, and local variables.
; =============================================================
;
;  With many arguments, or with recursion, registers aren't
;  enough. The caller PUSHES the arguments (last one first) and
;  then calls. Inside, the function sets up a FRAME:
;
;     push  ebp           ; save the caller's frame pointer
;     mov   ebp, esp      ; ebp = a fixed point in this frame
;     sub   esp, 4        ; room for one local variable
;
;  Now, no matter what is pushed later:
;
;     [ebp+12]   second argument
;     [ebp+8]    first argument
;     [ebp+4]    return address (pushed by call)
;     [ebp]      the caller's saved ebp
;     [ebp-4]    the first local variable
;
;  At the end:  mov esp, ebp / pop ebp / ret  (leave does the
;  first two). After the call the CALLER removes the arguments
;  with  add esp, 4 * (number of arguments).
;
;  This program uses main instead of _start: main is "called",
;  so it can end with ret, and its eax is the exit code.
; =============================================================

section .data
	t_area  db  "area(6, 7) = ", 0
	t_sum   db  "sum_to(100) = ", 0

section .text
global main

main:
	push	7                   ; second argument: height
	push	6                   ; first argument: width
	call	area                ; eax = 42
	add		esp, 8              ; remove the 2 arguments
	mov		esi, eax
	mov		eax, 202
	mov		ebx, t_area
	int		0x80
	mov		eax, 200
	mov		ebx, esi
	int		0x80
	mov		eax, 205
	int		0x80

	push	100
	call	sum_to              ; eax = 5050
	add		esp, 4
	mov		esi, eax
	mov		eax, 202
	mov		ebx, t_sum
	int		0x80
	mov		eax, 200
	mov		ebx, esi
	int		0x80
	mov		eax, 205
	int		0x80

	xor		eax, eax            ; return 0 = exit code 0
	ret

; ---- int area(int width, int height) ----
area:
	push	ebp
	mov		ebp, esp
	mov		eax, [ebp+8]        ; width
	imul	eax, [ebp+12]       ; * height
	pop		ebp
	ret

; ---- int sum_to(int n): 1 + 2 + ... + n, with a local ----
sum_to:
	push	ebp
	mov		ebp, esp
	sub		esp, 4              ; local: [ebp-4] = sum
	mov		dword [ebp-4], 0
	mov		ecx, [ebp+8]        ; ecx = n
.add:
	add		[ebp-4], ecx        ; sum += ecx
	loop	.add                ; n, n-1, ..., 1
	mov		eax, [ebp-4]        ; return sum
	leave                       ; = mov esp, ebp / pop ebp
	ret

; -------------------------------------------------------------
;  TRY IT
;   1. Step into area (F11) and look at the Stack panel: it
;      labels the arguments, the return address and the saved
;      EBP of each frame.
;   2. Forget  add esp, 8  after the first call. Everything is
;      printed, but the final ret of main crashes: the two
;      arguments are still on the stack, so ret takes the 6 as
;      its return address. Watch the Stack panel before that ret.
;   3. Write  int average(int a, int b, int c)  with three
;      arguments: [ebp+8], [ebp+12], [ebp+16].
;   4. Open the "Recursive factorial" example (File > Examples)
;      to see a function that calls itself.
;
;  Next: Lesson 13, strings.
; -------------------------------------------------------------
