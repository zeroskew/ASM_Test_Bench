; =============================================================
;  Lesson 17: Files
;  You will learn: opening, writing, appending, reading and
;  closing files, file descriptors, and checking for errors.
; =============================================================
;
;  A program asks the system to open a file and gets back a
;  FILE DESCRIPTOR (fd): a small number that names the open file.
;  0, 1 and 2 are already open: the keyboard, the console and the
;  console for errors. Files you open get 3, 4, ...
;
;     EAX = 5   open(EBX = name, ECX = flags)   -> EAX = fd
;     EAX = 4   write(EBX = fd, ECX = data, EDX = length)
;     EAX = 3   read(EBX = fd, ECX = buffer, EDX = room)
;               -> EAX = bytes read (0 = end of the file)
;     EAX = 19  lseek(EBX = fd, ECX = offset, EDX = from where)
;     EAX = 6   close(EBX = fd)
;
;  The flags say what you want; combine them with | :
;     O_RDONLY / O_WRONLY / O_RDWR   read, write or both
;     O_CREAT    create the file if it isn't there
;     O_TRUNC    empty it first       O_APPEND   add at the end
;
;  ERRORS: a call that fails returns a NEGATIVE number in EAX
;  (-2 = no such file, -13 = not allowed, ...). Always check:
;       cmp eax, 0  /  jl error
;  The Registers panel names the error next to EAX.
;
;  Files live in one folder: next to your .asm file once it is
;  saved, or Documents\ASM Test Bench for lessons and examples.
;  File > Open Files Folder shows it. A program can't reach
;  anything outside that folder.
; =============================================================

section .data
	name    db  "notes.txt", 0
	line1   db  "Assembly can write files!", 10
	len1    equ $ - line1
	line2   db  "This line was appended.", 10
	len2    equ $ - line2
	t_size  db  "File size: ", 0
	t_says  db  "The file says:", 10, 0
	t_error db  "File error ", 0

section .bss
	fd      resd 1              ; the file descriptor
	buf     resb 128            ; room for what we read

section .text
global _start

_start:
	; ---- 1. create the file and write a line ----
	mov		eax, 5              ; open
	mov		ebx, name
	mov		ecx, O_WRONLY | O_CREAT | O_TRUNC
	int		0x80
	cmp		eax, 0
	jl		error               ; negative: it failed
	mov		[fd], eax           ; keep the fd (3)

	mov		eax, 4              ; write(fd, line1, len1)
	mov		ebx, [fd]
	mov		ecx, line1
	mov		edx, len1
	int		0x80

	mov		eax, 6              ; close(fd)
	mov		ebx, [fd]
	int		0x80

	; ---- 2. open it again and add a line at the end ----
	mov		eax, 5
	mov		ebx, name
	mov		ecx, O_WRONLY | O_APPEND
	int		0x80
	cmp		eax, 0
	jl		error
	mov		[fd], eax
	mov		eax, 4
	mov		ebx, [fd]
	mov		ecx, line2
	mov		edx, len2
	int		0x80
	mov		eax, 6
	mov		ebx, [fd]
	int		0x80

	; ---- 3. read it back ----
	mov		eax, 5
	mov		ebx, name
	mov		ecx, O_RDONLY
	int		0x80
	cmp		eax, 0
	jl		error
	mov		[fd], eax

	mov		eax, 19             ; lseek(fd, 0, SEEK_END): jump to the
	mov		ebx, [fd]           ; end; the new position = the size
	mov		ecx, 0
	mov		edx, SEEK_END
	int		0x80
	mov		esi, eax
	mov		eax, 202
	mov		ebx, t_size
	int		0x80
	mov		eax, 200
	mov		ebx, esi            ; 50
	int		0x80
	mov		eax, 205
	int		0x80

	mov		eax, 19             ; back to the start: lseek(fd, 0, SEEK_SET)
	mov		ebx, [fd]
	mov		ecx, 0
	mov		edx, SEEK_SET
	int		0x80

	mov		eax, 3              ; read(fd, buf, 128)
	mov		ebx, [fd]
	mov		ecx, buf
	mov		edx, 128
	int		0x80                ; eax = bytes read
	mov		esi, eax

	mov		eax, 202
	mov		ebx, t_says
	int		0x80
	mov		eax, 4              ; print what was read: write(1, buf, n)
	mov		ebx, STDOUT
	mov		ecx, buf
	mov		edx, esi
	int		0x80

	mov		eax, 6
	mov		ebx, [fd]
	int		0x80

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; Prints "File error -N" and exits with code 1.
error:
	mov		esi, eax
	mov		eax, 202
	mov		ebx, t_error
	int		0x80
	mov		eax, 200
	mov		ebx, esi
	int		0x80
	mov		eax, 205
	int		0x80
	mov		eax, 1
	mov		ebx, 1
	int		0x80

; -------------------------------------------------------------
;  TRY IT
;   1. Run it, then File > Open Files Folder: notes.txt is there.
;   2. Step through with F10 and watch EAX after each int 0x80:
;      the fd, the bytes written, the size.
;   3. Change the name in the third open to "nothing.txt": EAX
;      becomes -2 and the Registers panel says ENOENT.
;   4. Try a name like "../secret.txt": -13 (EACCES). Programs
;      stay inside their folder.
;   5. Delete the file at the end: EAX = 10 (unlink), EBX = name.
;   6. Step Back stops at the last file operation: a file that
;      was written can't be "un-written".
;
;  The Reference tab lists more: creat, unlink, rename, mkdir,
;  rmdir, time, and brk for a heap.
; -------------------------------------------------------------
