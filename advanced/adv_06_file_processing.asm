; =============================================================
;  Advanced 6: Processing a file (a small "wc")
;  You will learn: reading a file in chunks until the end,
;  a state machine for counting words, folders (mkdir, rmdir),
;  and cleaning up (unlink) with error checks everywhere.
; =============================================================
;
;  Real files can be bigger than any buffer, so programs read
;  them in CHUNKS: read up to N bytes, process them, repeat until
;  read returns 0 (the end of the file):
;
;       loop:  eax = read(fd, buf, 16)
;              if eax == 0: done        if eax < 0: error
;              process buf[0 .. eax)
;
;  Counting words needs to remember whether we are inside a word
;  (a small STATE MACHINE): a word starts where a non-space
;  character follows a space (or the start).
;
;  The program writes a short poem into data/poem.txt, counts its
;  lines, words and characters, then deletes the file and the
;  folder again.
; =============================================================

section .data
	folder  db  "data", 0
	name    db  "data/poem.txt", 0
	poem    db  "The CPU is fast,", 10
			db  "the stack grows down,", 10
			db  "and every byte", 10
			db  "has an address.", 10
	plen    equ $ - poem
	t_lines db  "lines:  ", 0
	t_words db  "words:  ", 0
	t_chars db  "chars:  ", 0
	t_reads db  "chunks: ", 0
	t_clean db  "cleaned up", 10, 0
	t_error db  "file error ", 0

section .bss
	fd      resd 1
	buf     resb 16             ; deliberately small: several chunks
	lines   resd 1
	words   resd 1
	chars   resd 1
	chunks  resd 1

section .text
global _start

_start:
	; ---- a folder (it may exist already: -17 is fine) ----
	mov		eax, 39             ; mkdir
	mov		ebx, folder
	int		0x80
	cmp		eax, -17            ; EEXIST
	je		.write
	cmp		eax, 0
	jl		error

	; ---- write the poem ----
.write:
	mov		eax, 8              ; creat: create (or empty) for writing
	mov		ebx, name
	int		0x80
	cmp		eax, 0
	jl		error
	mov		[fd], eax
	mov		eax, 4
	mov		ebx, [fd]
	mov		ecx, poem
	mov		edx, plen
	int		0x80
	mov		eax, 6
	mov		ebx, [fd]
	int		0x80

	; ---- read it back, 16 bytes at a time ----
	mov		eax, 5
	mov		ebx, name
	mov		ecx, O_RDONLY
	int		0x80
	cmp		eax, 0
	jl		error
	mov		[fd], eax
	xor		edi, edi            ; state: 0 = between words, 1 = in a word
.chunk:
	mov		eax, 3              ; read(fd, buf, 16)
	mov		ebx, [fd]
	mov		ecx, buf
	mov		edx, 16
	int		0x80
	cmp		eax, 0
	jl		error
	je		.eof                ; 0 bytes: the end of the file
	inc		dword [chunks]
	add		[chars], eax
	xor		ecx, ecx            ; i = 0
.char:
	mov		dl, [buf + ecx]
	cmp		dl, 10
	jne		.not_newline
	inc		dword [lines]
.not_newline:
	cmp		dl, ' '
	je		.space
	cmp		dl, 10
	je		.space
	test	edi, edi            ; a letter: does a word start here?
	jnz		.in_word
	inc		dword [words]
	mov		edi, 1
	jmp		.in_word
.space:
	xor		edi, edi
.in_word:
	inc		ecx
	cmp		ecx, eax
	jl		.char
	jmp		.chunk
.eof:
	mov		eax, 6
	mov		ebx, [fd]
	int		0x80

	; ---- the counts ----
	mov		ebx, t_lines
	mov		esi, [lines]
	call	show                ; 4
	mov		ebx, t_words
	mov		esi, [words]
	call	show                ; 14
	mov		ebx, t_chars
	mov		esi, [chars]
	call	show                ; 70
	mov		ebx, t_reads
	mov		esi, [chunks]
	call	show                ; 5 (16 + 16 + 16 + 16 + 6)

	; ---- clean up: delete the file, then the (empty) folder ----
	mov		eax, 10             ; unlink
	mov		ebx, name
	int		0x80
	cmp		eax, 0
	jl		error
	mov		eax, 40             ; rmdir
	mov		ebx, folder
	int		0x80
	cmp		eax, 0
	jl		error
	mov		eax, 202
	mov		ebx, t_clean
	int		0x80

	mov		eax, 1              ; exit(0)
	mov		ebx, 0
	int		0x80

; show: the text at ebx, then the number in esi.
show:
	mov		eax, 202
	int		0x80
	mov		eax, 200
	mov		ebx, esi
	int		0x80
	mov		eax, 205
	int		0x80
	ret

; error: prints "file error -N" (eax = -N) and exits with code 1.
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
;   1. Make buf 4 bytes (and edx 4): more chunks, same counts.
;   2. Remove the cleanup and look in File > Open Files Folder.
;   3. Count the vowels too, or the longest line.
;   4. Copy the file: read chunks from one fd and write them to
;      another (opened with creat).
; -------------------------------------------------------------
