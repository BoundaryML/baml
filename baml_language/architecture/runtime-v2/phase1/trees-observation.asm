00000000000139a8 <baml_rust_program::f4180>:
   139a8:	d101c3ff 	sub	sp, sp, #0x70
   139ac:	a9057bfd 	stp	x29, x30, [sp, #80]
   139b0:	a9064ff4 	stp	x20, x19, [sp, #96]
   139b4:	910143fd 	add	x29, sp, #0x50
   139b8:	b00004e8 	adrp	x8, b0000 <__dso_handle>
   139bc:	39696108 	ldrb	w8, [x8, #2648]
   139c0:	350006c8 	cbnz	w8, 13a98 <baml_rust_program::f4180+0xf0>
   139c4:	d53bd048 	mrs	x8, tpidr_el0
   139c8:	910013e9 	add	x9, sp, #0x4
   139cc:	390013ff 	strb	wzr, [sp, #4]
   139d0:	91400108 	add	x8, x8, #0x0, lsl #12
   139d4:	91026108 	add	x8, x8, #0x98
   139d8:	f9400108 	ldr	x8, [x8]
   139dc:	eb080128 	subs	x8, x9, x8
   139e0:	9a8833e8 	csel	x8, xzr, x8, cc	// cc = lo, ul, last
   139e4:	f140411f 	cmp	x8, #0x10, lsl #12
   139e8:	54000643 	b.cc	13ab0 <baml_rust_program::f4180+0x108>  // b.lo, b.ul, b.last
   139ec:	f1000414 	subs	x20, x0, #0x1
   139f0:	5400032b 	b.lt	13a54 <baml_rust_program::f4180+0xac>  // b.tstop
   139f4:	d2800028 	mov	x8, #0x1                   	// #1
   139f8:	f2e80008 	movk	x8, #0x4000, lsl #48
   139fc:	eb08001f 	cmp	x0, x8
   13a00:	540003c2 	b.cs	13a78 <baml_rust_program::f4180+0xd0>  // b.hs, b.nlast
   13a04:	aa1403e0 	mov	x0, x20
   13a08:	97ffffe8 	bl	139a8 <baml_rust_program::f4180>
   13a0c:	f100041f 	cmp	x0, #0x1
   13a10:	aa0103f3 	mov	x19, x1
   13a14:	54000620 	b.eq	13ad8 <baml_rust_program::f4180+0x130>  // b.none
