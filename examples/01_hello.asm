; =============================================================
;  Hello, World!
;  Prints a message using the Linux 'write' system call.
;  Try: press F10 to step through it line by line and watch
;  the registers on the right change.
; =============================================================

section .data
	msg     db  "Hello, World!", 10     ; 10 = newline
	len     equ $ - msg                 ; length = here - start of msg

section .text
global _start

_start:
	mov		eax, 4          ; system call 4 = write
	mov		ebx, 1          ; file descriptor 1 = stdout
	mov		ecx, msg        ; address of the text
	mov		edx, len        ; number of bytes
	int		0x80            ; call the "kernel"

	mov		eax, 1          ; system call 1 = exit
	mov		ebx, 0          ; exit code 0
	int		0x80
