#![allow(unused_mut,unused_variables,unused_assignments,unused_labels,dead_code,non_snake_case)]
use bex_v2_runtime::{self as rt, R};
use bex_v2_heap::{Descriptor,Kind,Field,SlotKind};
static DESCRIPTORS: &[Descriptor] = &[rt::STRING_DESC,rt::OVERFLOW_DESC,rt::SHIFT_DESC,rt::CANCEL_DESC,rt::STACK_DESC,
Descriptor{kind:Kind::Class,size:24,ptr_slots:(8,2),tagged_slots:(0,0),elem:None,type_id:5,fields:&[Field{name:"left",offset:8,slot:SlotKind::Pointer,type_id:5},Field{name:"right",offset:16,slot:SlotKind::Pointer,type_id:5}],finalizer:None},
];
#[repr(C)] struct C4179{header:bex_v2_heap::Header,f0:usize,f1:usize,}
static O4180:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4180:rt::Source=rt::Source{file:"main.baml",function:"user.bottom_up",lines:&[6,6,6,6,7,7,7,7,7,7,7,7,6,6]};
unsafe fn f4180(mut l1:i64)->R<usize>{unsafe{rt::observe(&O4180);rt::check_stack().map_err(|e|e.at(&S4180,0))?;
let mut l0:usize=0;
let mut l2:bool=false;
let mut l3:usize=0;
let mut l4:i64=0;
let mut l5:usize=0;
let mut l6:i64=0;
let mut c0=false;
'b5:{ l2=(l1 <= 0i64);
c0=l2;
if c0{ l0={let a0=0usize;let a1=0usize;let p=rt::alloc(5, 24) as *mut C4179;bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f0).cast(),a0);bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f1).cast(),a1);p as usize};
break 'b5;
 }else{ l4=(rt::sub(l1,1i64)).map_err(|e|e.at(&S4180,5))?;
l3=(f4180(l4)).map_err(|e|e.at(&S4180,6))?;
l6=(rt::sub(l1,1i64)).map_err(|e|e.at(&S4180,8))?;
l5=(f4180(l6)).map_err(|e|e.at(&S4180,9))?;
l0={let a0=l3;let a1=l5;let p=rt::alloc(5, 24) as *mut C4179;bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f0).cast(),a0);bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f1).cast(),a1);p as usize};
break 'b5;
 }
 }
return Ok(l0);
}}
static O4181:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4181:rt::Source=rt::Source{file:"main.baml",function:"user.check",lines:&[10,10,11,12,12,12,12,12,12,12,12,13,12,13,13,13,13,13,13,13,12,12,12,12]};
unsafe fn f4181(mut l1:usize)->R<i64>{unsafe{rt::observe(&O4181);rt::check_stack().map_err(|e|e.at(&S4181,0))?;
let mut l0:i64=0;
let mut l2:usize=0;
let mut l3:usize=0;
let mut l4:bool=false;
let mut l5:usize=0;
let mut l6:bool=false;
let mut l7:usize=0;
let mut l8:i64=0;
let mut l9:i64=0;
let mut l10:usize=0;
let mut l11:i64=0;
let mut l12:usize=0;
let mut l13:i64=0;
let mut c1=false;
let mut c3=false;
l2=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4181,1))? as *const C4179)).f0;
l3=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4181,2))? as *const C4179)).f1;
l5=l2;
l4=(l5==0usize);
'b8:{ 'b7:{ c1=l4;
if c1{ break 'b7;
 }else{ l7=l3;
l6=(l7==0usize);
c3=l6;
if c3{ break 'b7;
 }else{ l10=l2;
l13=1i64;
l9=(f4181(l10)).map_err(|e|e.at(&S4181,13))?;
l8=(rt::add(l13,l9)).map_err(|e|e.at(&S4181,15))?;
l12=l3;
l11=(f4181(l12)).map_err(|e|e.at(&S4181,17))?;
l0=(rt::add(l8,l11)).map_err(|e|e.at(&S4181,19))?;
break 'b8;
 }
 }
 }
l0=1i64;
break 'b8;
 }
return Ok(l0);
}}
static O4182:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4182:rt::Source=rt::Source{file:"main.baml",function:"user.main",lines:&[17,17,17,17,18,18,19,19,19,19,19,20,20,20,20,20,21,22,22,22,22,23,23,23,23,23,23,23,22,22,25,25,19,19,27,27,27,27,27,27,27]};
unsafe fn f4182()->R<i64>{unsafe{rt::observe(&O4182);rt::check_stack().map_err(|e|e.at(&S4182,0))?;
let mut l0:i64=0;
let mut l1:i64=0;
let mut l2:usize=0;
let mut l3:usize=0;
let mut l4:i64=0;
let mut l5:bool=false;
let mut l6:i64=0;
let mut l7:i64=0;
let mut l8:i64=0;
let mut l9:i64=0;
let mut l10:i64=0;
let mut l11:i64=0;
let mut l12:i64=0;
let mut l13:bool=false;
let mut l14:i64=0;
let mut l15:i64=0;
let mut l16:i64=0;
let mut l17:usize=0;
let mut l18:i64=0;
let mut l19:i64=0;
let mut l20:i64=0;
let mut l21:i64=0;
let mut l22:usize=0;
let mut c4=false;
let mut c9=false;
let mut poll4=0u32;
let mut poll9=0u32;
l2=(f4180(15i64)).map_err(|e|e.at(&S4182,1))?;
l1=(f4181(l2)).map_err(|e|e.at(&S4182,3))?;
l3=(f4180(14i64)).map_err(|e|e.at(&S4182,5))?;
l4=4i64;
'l4:loop{ poll4=poll4.wrapping_add(1);if poll4&15==0{(rt::poll()).map_err(|e|e.at(&S4182,8))?;}
l6=l4;
l5=(l6 <= 14i64);
c4=l5;
if c4{ l10=l4;
l9=(rt::sub(14i64,l10)).map_err(|e|e.at(&S4182,13))?;
l8=(rt::add(l9,4i64)).map_err(|e|e.at(&S4182,14))?;
l7=(rt::shl(1i64,l8)).map_err(|e|e.at(&S4182,15))?;
l11=0i64;
l12=0i64;
'l9:loop{ poll9=poll9.wrapping_add(1);if poll9&15==0{(rt::poll()).map_err(|e|e.at(&S4182,18))?;}
l14=l12;
l15=l7;
l13=(l14 < l15);
c9=l13;
if c9{ l18=l4;
l17=(f4180(l18)).map_err(|e|e.at(&S4182,23))?;
l16=(f4181(l17)).map_err(|e|e.at(&S4182,25))?;
l11=(rt::add(l11,l16)).map_err(|e|e.at(&S4182,27))?;
l12=(rt::add(l12,1i64)).map_err(|e|e.at(&S4182,29))?;
continue 'l9;
 }else{ break 'l9;
 }
 }
l19=l11;
l1=(rt::add(l1,l19)).map_err(|e|e.at(&S4182,31))?;
l4=(rt::add(l4,2i64)).map_err(|e|e.at(&S4182,33))?;
continue 'l4;
 }else{ break 'l4;
 }
 }
l20=l1;
l22=l3;
l21=(f4181(l22)).map_err(|e|e.at(&S4182,37))?;
l0=(rt::add(l20,l21)).map_err(|e|e.at(&S4182,38))?;
return Ok(l0);
}}
fn entry()->R<i64>{unsafe{f4182()}}
fn main()->std::process::ExitCode{O4180.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);O4181.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);O4182.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);rt::driver(DESCRIPTORS,entry)}
