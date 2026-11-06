 Percent |	Source code & Disassembly of richards-v2 for armv8_pmuv3_0/cycles/u (3082 samples, percent: local period)
-------------------------------------------------------------------------------------------------------------------------
         :
         :
         :
         : 3     Disassembly of section .text:
         :
         : 5     00000000000128ec <baml_rust_program::entry>:
         : 6     }
         : 7     }
         : 8     l0=l1;
         : 9     return Ok(l0);
         : 10    }}
         : 11    fn entry()->R<i64>{unsafe{f4203()}}
    0.00 :   128ec:  stp     x29, x30, [sp, #-96]!
    0.00 :   128f0:  stp     x28, x27, [sp, #16]
    0.00 :   128f4:  stp     x26, x25, [sp, #32]
    0.00 :   128f8:  stp     x24, x23, [sp, #48]
    0.00 :   128fc:  stp     x22, x21, [sp, #64]
    0.00 :   12900:  stp     x20, x19, [sp, #80]
    0.00 :   12904:  mov     x29, sp
    0.00 :   12908:  sub     sp, sp, #0x2d0
         : 20    #[cfg_attr(miri, track_caller)] // even without panics, this helps for Miri backtraces
         : 21    unsafe fn atomic_load<T: Copy>(dst: *const T, order: Ordering) -> T {
         : 22    // SAFETY: the caller must uphold the safety contract for `atomic_load`.
         : 23    unsafe {
         : 24    match order {
         : 25    Relaxed => intrinsics::atomic_load::<T, { AO::Relaxed }>(dst),
    0.00 :   1290c:  adrp    x8, b0000 <__dso_handle>
    0.00 :   12910:  ldrb    w8, [x8, #2708]
         : 28    ))
         : 29    }
         : 30    }
         : 31    #[inline(always)]
         : 32    pub fn observe(word: &AtomicU8) {
         : 33    if word.load(Ordering::Relaxed) != 0 {
    0.00 :   12914:  cbnz    w8, 14018 <baml_rust_program::entry+0x172c>
         : 35    } else {
         : 36    |_| {
         : 37    #[thread_local]
         : 38    $(#[$align_attr])*
         : 39    static __RUST_STD_INTERNAL_VAL: $t = __RUST_STD_INTERNAL_INIT;
         : 40    &__RUST_STD_INTERNAL_VAL
    0.00 :   12918:  mrs     x9, tpidr_el0
    0.00 :   1291c:  sub     x26, x29, #0x28
         : 43    let marker = 0u8;
    0.00 :   12920:  sturb   wzr, [x29, #-40]
    0.00 :   12924:  add     x8, x9, #0x0, lsl #12
    0.00 :   12928:  add     x8, x8, #0x98
    0.00 :   1292c:  str     x8, [sp, #184]
         : 48    pub fn try_with<F, R>(&'static self, f: F) -> Result<R, AccessError>
         : 49    where
         : 50    F: FnOnce(&T) -> R,
         : 51    {
         : 52    let thread_local = unsafe { (self.inner)(None).as_ref().ok_or(AccessError)? };
         : 53    Ok(f(thread_local))
    0.00 :   12930:  ldr     x8, [x8]
         : 55    #[must_use = "this returns the result of the operation, \
         : 56    without modifying the original"]
         : 57    #[rustc_const_stable(feature = "const_saturating_int_methods", since = "1.47.0")]
         : 58    #[inline(always)]
         : 59    pub const fn saturating_sub(self, rhs: Self) -> Self {
         : 60    intrinsics::saturating_sub(self, rhs)
    0.00 :   12934:  subs    x8, x26, x8
    0.00 :   12938:  csel    x8, xzr, x8, cc // cc = lo, ul, last
         : 63    if (&raw const marker as usize).saturating_sub(crate::STACK_LIMIT.get()) < 64 * 1024 {
    0.00 :   1293c:  cmp     x8, #0x10, lsl #12
    0.00 :   12940:  b.cc    141b4 <baml_rust_program::entry+0x18c8>  // b.lo, b.ul, b.last
    0.00 :   12944:  add     x9, x9, #0x0, lsl #12
    0.00 :   12948:  mov     w10, #0x1                       // #1
    0.00 :   1294c:  adrp    x11, 7d000 <__aarch64_ldadd8_rel+0x10>
    0.00 :   12950:  add     x24, x9, #0xb8
    0.00 :   12954:  adrp    x9, 7d000 <__aarch64_ldadd8_rel+0x10>
    0.00 :   12958:  mov     x23, xzr
    0.00 :   1295c:  ldr     q0, [x9, #1088]
    0.00 :   12960:  adrp    x9, 7d000 <__aarch64_ldadd8_rel+0x10>
    0.00 :   12964:  mov     x22, xzr
    0.00 :   12968:  ldr     q1, [x9, #1104]
    0.00 :   1296c:  adrp    x9, 7d000 <__aarch64_ldadd8_rel+0x10>
    0.00 :   12970:  mov     x12, #0xffffffffffffffff        // #-1
    0.00 :   12974:  str     q0, [sp, #160]
    0.00 :   12978:  dup     v0.2d, x10
    0.00 :   1297c:  adrp    x10, 7d000 <__aarch64_ldadd8_rel+0x10>
    0.00 :   12980:  adrp    x21, b0000 <__dso_handle>
    0.00 :   12984:  mov     w20, #0x6                       // #6
    0.00 :   12988:  stp     q0, q1, [sp, #128]
    0.00 :   1298c:  ldr     q0, [x9, #1120]
    0.00 :   12990:  adrp    x9, 7d000 <__aarch64_ldadd8_rel+0x10>
    0.00 :   12994:  ldr     q1, [x11, #1200]
    0.00 :   12998:  str     q0, [sp, #112]
    0.00 :   1299c:  ldr     q0, [x9, #1136]
    0.00 :   129a0:  adrp    x9, 7d000 <__aarch64_ldadd8_rel+0x10>
    0.00 :   129a4:  str     q1, [sp, #32]
    0.00 :   129a8:  str     q0, [sp, #96]
    0.00 :   129ac:  ldr     q0, [x9, #1152]
    0.00 :   129b0:  adrp    x9, 7d000 <__aarch64_ldadd8_rel+0x10>
    0.00 :   129b4:  str     q0, [sp, #80]
    0.00 :   129b8:  ldr     q0, [x9, #1168]
    0.00 :   129bc:  str     q0, [sp, #48]
    0.00 :   129c0:  ldr     q0, [x10, #1184]
    0.00 :   129c4:  str     q0, [sp, #64]
         : 99    #[rustc_const_stable(feature = "const_int_methods", since = "1.32.0")]
         : 100   #[must_use = "this returns the result of the operation, \
         : 101   without modifying the original"]
         : 102   #[inline(always)]
         : 103   pub const fn wrapping_add(self, rhs: Self) -> Self {
         : 104   intrinsics::wrapping_add(self, rhs)
    0.00 :   129c8:  add     x25, x22, #0x1
         : 106   'l1:loop{ poll1=poll1.wrapping_add(1);if poll1&15==0{(rt::poll()).map_err(|e|e.at(&S4203,3))?;}
    0.00 :   129cc:  tst     x25, #0xf
    0.00 :   129d0:  b.ne    129e8 <baml_rust_program::entry+0xfc>  // b.any
    0.00 :   129d4:  ldr     x8, [x24]
         : 110   /// The same rooting contract as `alloc` applies. This phase-one adapter supports
         : 111   /// a single root task; it does not expose child spawning or heap-valued results.
         : 112   #[inline(always)]
         : 113   pub unsafe fn safepoint() -> Result<(), bex_v2_loop::Err> {
         : 114   let word = POLL_WORD.get();
         : 115   assert!(!word.is_null(), "v2 safepoint outside a root task");
    0.00 :   129d8:  cbz     x8, 142fc <baml_rust_program::entry+0x1a10>
         : 117   #[rustc_const_stable(feature = "const_cell", since = "1.88.0")]
         : 118   #[rustc_should_not_be_called_on_const_items]
         : 119   pub const fn get(&self) -> T {
         : 120   // SAFETY: This can cause data races if called from a separate thread,
         : 121   // but `Cell` is `!Sync` so this won't happen.
         : 122   unsafe { *self.value.get() }
    0.00 :   129dc:  ldrb    w8, [x8]
         : 124   if unsafe { (*word).get() } == 0 {
    0.00 :   129e0:  cbnz    w8, 13e70 <baml_rust_program::entry+0x1584>
         : 126   return Ok(());
    0.00 :   129e4:  str     x12, [sp, #192]
         : 128   if c1{ l5=(f4202()).map_err(|e|e.at(&S4203,7))?;
    0.00 :   129e8:  cmp     x22, #0x32
    0.00 :   129ec:  b.eq    13f80 <baml_rust_program::entry+0x1694>  // b.none
    0.00 :   129f0:  ldrb    w8, [x21, #2704]
         : 132   if word.load(Ordering::Relaxed) != 0 {
    0.00 :   129f4:  cbnz    w8, 14018 <baml_rust_program::entry+0x172c>
    0.00 :   129f8:  ldr     x8, [sp, #184]
         : 135   let marker = 0u8;
    0.00 :   129fc:  sturb   wzr, [x29, #-40]
    0.00 :   12a00:  ldr     x8, [x8]
         : 738   {let value=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4202,51))? as *const C4181)).f3;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4202,51))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4181)).f4,value);}
    0.00 :   13190:  ldr     x9, [x21]
    0.00 :   13194:  b       1319c <baml_rust_program::entry+0x8b0>
         : 741   if c18{ {let value=(*((rt::nonnull(l30)).map_err(|e|e.at(&S4202,67))? as *const C4180)).f0;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4202,67))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4181)).f4,value);}
    0.72 :   13198:  ldr     x9, [x9, #8]
         : 743   #[rustc_const_stable(feature = "const_wrapping_math", since = "1.32.0")]
         : 744   #[must_use = "this returns the result of the operation, \
         : 745   without modifying the original"]
         : 746   #[inline(always)]
         : 747   pub const fn wrapping_add(self, rhs: Self) -> Self {
         : 748   intrinsics::wrapping_add(self, rhs)
    0.52 :   1319c:  add     w19, w19, #0x1
    0.73 :   131a0:  str     x9, [x20, #24]
         : 751   'l15:loop{ poll15=poll15.wrapping_add(1);if poll15&15==0{(rt::poll()).map_err(|e|e.at(&S4202,52))?;}
    0.95 :   131a4:  tst     w19, #0xf
    0.00 :   131a8:  b.eq    131c0 <baml_rust_program::entry+0x8d4>  // b.none
    0.75 :   131ac:  ldr     x24, [sp, #16]
    0.07 :   131b0:  mov     x12, #0xffffffffffffffff        // #-1
    0.29 :   131b4:  adrp    x21, b0000 <__dso_handle>
         : 757   if c16{ l39=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4202,56))? as *const C4181)).f0;
    0.35 :   131b8:  cbnz    x9, 131e4 <baml_rust_program::entry+0x8f8>
    0.00 :   131bc:  b       13e24 <baml_rust_program::entry+0x1538>
    0.09 :   131c0:  ldr     x24, [sp, #16]
    0.00 :   131c4:  mov     x12, #0xffffffffffffffff        // #-1
    2.22 :   131c8:  adrp    x21, b0000 <__dso_handle>
    0.10 :   131cc:  ldr     x8, [x24]
         : 764   assert!(!word.is_null(), "v2 safepoint outside a root task");
    1.47 :   131d0:  cbz     x8, 142fc <baml_rust_program::entry+0x1a10>
    0.33 :   131d4:  ldrb    w8, [x8]
         : 767   if unsafe { (*word).get() } == 0 {
    1.00 :   131d8:  cbnz    w8, 13dec <baml_rust_program::entry+0x1500>
         : 769   return Ok(());
    0.48 :   131dc:  str     x12, [sp, #240]
    0.13 :   131e0:  cbz     x9, 13e24 <baml_rust_program::entry+0x1538>
    1.41 :   131e4:  ldrb    w8, [x14, #2664]
         : 773   if word.load(Ordering::Relaxed) != 0 {
    0.84 :   131e8:  cbnz    w8, 14018 <baml_rust_program::entry+0x172c>
    0.90 :   131ec:  ldr     x8, [sp, #184]
         : 776   let marker = 0u8;
    0.91 :   131f0:  sturb   wzr, [x29, #-40]
    0.66 :   131f4:  ldr     x8, [x8]
         : 779   intrinsics::saturating_sub(self, rhs)
    0.42 :   131f8:  subs    x10, x26, x8
    1.17 :   131fc:  csel    x10, xzr, x10, cc       // cc = lo, ul, last
         : 782   if (&raw const marker as usize).saturating_sub(crate::STACK_LIMIT.get()) < 64 * 1024 {
