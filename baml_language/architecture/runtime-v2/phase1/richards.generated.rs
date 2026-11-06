#![allow(unused_mut,unused_variables,unused_assignments,unused_labels,dead_code,non_snake_case)]
use bex_v2_runtime::{self as rt, R};
use bex_v2_heap::{Descriptor,Kind,Field,SlotKind};
static DESCRIPTORS: &[Descriptor] = &[rt::STRING_DESC,rt::OVERFLOW_DESC,rt::SHIFT_DESC,rt::CANCEL_DESC,rt::STACK_DESC,rt::ALLOC_DESC,rt::INDEX_DESC,
Descriptor{kind:Kind::Class,size:48,ptr_slots:(8,2),tagged_slots:(24,0),elem:None,type_id:10,fields:&[Field{name:"link",offset:8,slot:SlotKind::Pointer,type_id:10},Field{name:"id",offset:24,slot:SlotKind::Scalar,type_id:0},Field{name:"kind",offset:32,slot:SlotKind::Scalar,type_id:0},Field{name:"a1",offset:40,slot:SlotKind::Scalar,type_id:0},Field{name:"a2",offset:16,slot:SlotKind::Pointer,type_id:17}],finalizer:None},
Descriptor{kind:Kind::Class,size:64,ptr_slots:(8,2),tagged_slots:(24,1),elem:None,type_id:11,fields:&[Field{name:"link",offset:8,slot:SlotKind::Pointer,type_id:11},Field{name:"id",offset:40,slot:SlotKind::Scalar,type_id:0},Field{name:"priority",offset:48,slot:SlotKind::Scalar,type_id:0},Field{name:"queue",offset:16,slot:SlotKind::Pointer,type_id:10},Field{name:"task",offset:24,slot:SlotKind::Tagged,type_id:4205},Field{name:"state",offset:56,slot:SlotKind::Scalar,type_id:0}],finalizer:None},
Descriptor{kind:Kind::Class,size:56,ptr_slots:(8,3),tagged_slots:(32,0),elem:None,type_id:12,fields:&[Field{name:"queue_count",offset:32,slot:SlotKind::Scalar,type_id:0},Field{name:"hold_count",offset:40,slot:SlotKind::Scalar,type_id:0},Field{name:"blocks",offset:8,slot:SlotKind::Pointer,type_id:18},Field{name:"list",offset:16,slot:SlotKind::Pointer,type_id:11},Field{name:"current",offset:24,slot:SlotKind::Pointer,type_id:11},Field{name:"current_id",offset:48,slot:SlotKind::Scalar,type_id:0}],finalizer:None},
Descriptor{kind:Kind::Class,size:24,ptr_slots:(8,0),tagged_slots:(8,0),elem:None,type_id:13,fields:&[Field{name:"v1",offset:8,slot:SlotKind::Scalar,type_id:0},Field{name:"count",offset:16,slot:SlotKind::Scalar,type_id:0}],finalizer:None},
Descriptor{kind:Kind::Class,size:16,ptr_slots:(8,1),tagged_slots:(16,0),elem:None,type_id:14,fields:&[Field{name:"v1",offset:8,slot:SlotKind::Pointer,type_id:10}],finalizer:None},
Descriptor{kind:Kind::Class,size:24,ptr_slots:(8,0),tagged_slots:(8,0),elem:None,type_id:15,fields:&[Field{name:"v1",offset:8,slot:SlotKind::Scalar,type_id:0},Field{name:"v2",offset:16,slot:SlotKind::Scalar,type_id:0}],finalizer:None},
Descriptor{kind:Kind::Class,size:24,ptr_slots:(8,2),tagged_slots:(24,0),elem:None,type_id:16,fields:&[Field{name:"v1",offset:8,slot:SlotKind::Pointer,type_id:10},Field{name:"v2",offset:16,slot:SlotKind::Pointer,type_id:10}],finalizer:None},
Descriptor{kind:Kind::List,size:8,ptr_slots:(0,0),tagged_slots:(0,0),elem:Some(bex_v2_heap::Elements{offset:16,layout:bex_v2_heap::ElementLayout::Scalar}),type_id:17,fields:&[],finalizer:None},
Descriptor{kind:Kind::List,size:8,ptr_slots:(0,0),tagged_slots:(0,0),elem:Some(bex_v2_heap::Elements{offset:16,layout:bex_v2_heap::ElementLayout::Pointer}),type_id:18,fields:&[],finalizer:None},
];
#[repr(C)] struct C4179{header:bex_v2_heap::Header,f0:usize,f4:usize,f1:i64,f2:i64,f3:i64,}
#[repr(C)] struct C4180{header:bex_v2_heap::Header,f0:usize,f3:usize,f4:rt::Dyn,f1:i64,f2:i64,f5:i64,}
#[repr(C)] struct C4181{header:bex_v2_heap::Header,f2:usize,f3:usize,f4:usize,f0:i64,f1:i64,f5:i64,}
#[repr(C)] struct C4182{header:bex_v2_heap::Header,f0:i64,f1:i64,}
#[repr(C)] struct C4183{header:bex_v2_heap::Header,f0:usize,}
#[repr(C)] struct C4184{header:bex_v2_heap::Header,f0:i64,f1:i64,}
#[repr(C)] struct C4185{header:bex_v2_heap::Header,f0:usize,f1:usize,}
static O4187:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4187:rt::Source=rt::Source{file:"main.baml",function:"user.packet",lines:&[14,14,14,14]};
#[inline(always)]
unsafe fn f4187(mut l1:usize,mut l2:i64,mut l3:i64)->R<usize>{unsafe{rt::observe(&O4187);rt::check_stack().map_err(|e|e.at(&S4187,0))?;
let mut l0:usize=0;
let mut l4:usize=0;
l4={let a0=0i64;let a1=0i64;let a2=0i64;let a3=0i64;let p=(rt::try_alloc(14,48)).map_err(|e|e.at(&S4187,0))?;p.add(8).cast::<usize>().write(4);p.add(16).cast::<i64>().write(a0);p.add(24).cast::<i64>().write(a1);p.add(32).cast::<i64>().write(a2);p.add(40).cast::<i64>().write(a3);p as usize};
l0={let a0=l1;let a1=l2;let a2=l3;let a3=0i64;let a4=l4;let p=(rt::try_alloc(7, 48)).map_err(|e|e.at(&S4187,1))? as *mut C4179;bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f0).cast(),a0);(*p).f1=a1 as _;(*p).f2=a2 as _;(*p).f3=a3 as _;bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f4).cast(),a4);p as usize};
return Ok(l0);
}}
static O4188:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4188:rt::Source=rt::Source{file:"main.baml",function:"user.add_to",lines:&[17,17,18,18,18,18,19,19,21,21,22,22,25,25,26,23,23,18,18]};
#[inline(always)]
unsafe fn f4188(mut l1:usize,mut l2:usize)->R<usize>{unsafe{rt::observe(&O4188);rt::check_stack().map_err(|e|e.at(&S4188,0))?;
let mut l0:usize=0;
let mut l3:bool=false;
let mut l4:usize=0;
let mut l5:usize=0;
let mut l6:bool=false;
let mut l7:usize=0;
let mut c1=false;
let mut c4=false;
let mut poll3=0u32;
{let value=0usize;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4188,1))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4179)).f0,value);}
l3=(l2==0usize);
'b8:{ c1=l3;
if c1{ l0=l1;
break 'b8;
 }else{ l4=l2;
'l3:loop{ poll3=poll3.wrapping_add(1);if poll3&15==0{(rt::poll()).map_err(|e|e.at(&S4188,8))?;}
l5=(*((rt::nonnull(l4)).map_err(|e|e.at(&S4188,9))? as *const C4179)).f0;
l7=l5;
l6=(l7==0usize);
c4=l6;
if c4{ {let value=l1;let object=(rt::nonnull(l4)).map_err(|e|e.at(&S4188,13))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4179)).f0,value);}
l0=l2;
break 'b8;
 }else{ l4=l5;
continue 'l3;
 }
 }
 }
 }
return Ok(l0);
}}
static O4190:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4190:rt::Source=rt::Source{file:"main.baml",function:"user.tcb_run",lines:&[40,40,41,41,42,42,43,43,43,44,44,45,46,46,46,46,46,46,46,49,49,49,49,49]};
#[inline(always)]
unsafe fn f4190(mut l1:usize,mut l2:usize)->R<usize>{unsafe{rt::observe(&O4190);rt::check_stack().map_err(|e|e.at(&S4190,0))?;
let mut l0:usize=0;
let mut l3:usize=0;
let mut l4:bool=false;
let mut l5:i64=0;
let mut l6:usize=0;
let mut l7:bool=false;
let mut l8:usize=0;
let mut l9:bool=false;
let mut l10:bool=false;
let mut l11:usize=0;
let mut l12:rt::Dyn=rt::Dyn{tag:2,payload:0};
let mut l13:usize=0;
let mut c0=false;
let mut c2=false;
let mut c4=false;
'b7:{ l3=0usize;
l5=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4190,2))? as *const C4180)).f5;
l4=(l5 == 3i64);
c0=l4;
if c0{ l6=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4190,5))? as *const C4180)).f3;
l8=l6;
l9=(l8==0usize);
l7=!(l9);
c2=l7;
if c2{ l3=l6;
{let value=(*((rt::nonnull(l6)).map_err(|e|e.at(&S4190,11))? as *const C4179)).f0;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4190,11))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4180)).f3,value);}
l11=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4190,12))? as *const C4180)).f3;
l10=(l11==0usize);
c4=l10;
if c4{ {let value=0i64;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4190,16))?;(&raw mut (*(object as *mut C4180)).f5).write(value as _);}
break 'b7;
 }else{ {let value=1i64;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4190,18))?;(&raw mut (*(object as *mut C4180)).f5).write(value as _);}
break 'b7;
 }
 }else{ break 'b7;
 }
 }else{ break 'b7;
 }
 }
l12=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4190,20))? as *const C4180)).f4;
l13=l3;
l0=({let a0=l12;let a1=l2;let a2=l13;match a0.tag{13=>f4198(a0.payload as usize,a1,a2),14=>f4199(a0.payload as usize,a1,a2),15=>f4200(a0.payload as usize,a1,a2),16=>f4201(a0.payload as usize,a1,a2),_=>Err(rt::Error::internal("invalid closed-world interface tag".into()))}}).map_err(|e|e.at(&S4190,21))?;
return Ok(l0);
}}
static O4191:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4191:rt::Source=rt::Source{file:"main.baml",function:"user.check_priority_add",lines:&[52,52,52,52,53,53,54,54,55,55,55,55,55,57,57,57,57,57,59,59,55,55]};
#[inline(always)]
unsafe fn f4191(mut l1:usize,mut l2:usize,mut l3:usize)->R<usize>{unsafe{rt::observe(&O4191);rt::check_stack().map_err(|e|e.at(&S4191,0))?;
let mut l0:usize=0;
let mut l4:bool=false;
let mut l5:usize=0;
let mut l6:i64=0;
let mut l7:bool=false;
let mut l8:i64=0;
let mut l9:i64=0;
let mut l10:usize=0;
let mut l11:usize=0;
let mut c1=false;
let mut c4=false;
l5=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4191,1))? as *const C4180)).f3;
l4=(l5==0usize);
'b7:{ 'b5:{ c1=l4;
if c1{ {let value=l3;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4191,5))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4180)).f3,value);}
l6=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4191,6))? as *const C4180)).f5;
{let value=(l6 | 1i64);let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4191,7))?;(&raw mut (*(object as *mut C4180)).f5).write(value as _);}
l8=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4191,8))? as *const C4180)).f2;
l9=(*((rt::nonnull(l2)).map_err(|e|e.at(&S4191,9))? as *const C4180)).f2;
l7=(l8 > l9);
c4=l7;
if c4{ l0=l1;
break 'b7;
 }else{ break 'b5;
 }
 }else{ l10=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4191,14))? as *const C4180)).f3;
l11=(f4188(l3,l10)).map_err(|e|e.at(&S4191,15))?;
{let value=l11;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4191,17))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4180)).f3,value);}
break 'b5;
 }
 }
l0=l2;
break 'b7;
 }
return Ok(l0);
}}
static O4192:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4192:rt::Source=rt::Source{file:"main.baml",function:"user.held_or_suspended",lines:&[62,62,62,62,62,62,62,62,62]};
#[inline(always)]
unsafe fn f4192(mut l1:usize)->R<bool>{unsafe{rt::observe(&O4192);rt::check_stack().map_err(|e|e.at(&S4192,0))?;
let mut l0:bool=false;
let mut l2:bool=false;
let mut l3:bool=false;
let mut l4:i64=0;
let mut l5:i64=0;
let mut l6:i64=0;
let mut c0=false;
'b2:{ l5=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4192,1))? as *const C4180)).f5;
l4=(l5 & 4i64);
l3=(l4 != 0i64);
c0=l3;if c0{l2=l3;
}
if c0{ break 'b2;
 }else{ l6=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4192,5))? as *const C4180)).f5;
l2=(l6 == 2i64);
break 'b2;
 }
 }
l0=l2;
return Ok(l0);
}}
static O4193:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4193:rt::Source=rt::Source{file:"main.baml",function:"user.add_task",lines:&[73,73,73,73,73,73,73,74,74,74,74,75,76,77,77,78,78,78]};
#[inline(always)]
unsafe fn f4193(mut l1:usize,mut l2:i64,mut l3:i64,mut l4:usize,mut l5:rt::Dyn)->R<usize>{unsafe{rt::observe(&O4193);rt::check_stack().map_err(|e|e.at(&S4193,0))?;
let mut l0:usize=0;
let mut l6:i64=0;
let mut l7:bool=false;
let mut l8:usize=0;
let mut l9:usize=0;
let mut l10:i64=0;
let mut l11:usize=0;
let mut l12:i64=0;
let mut c1=false;
l7=(l4==0usize);
'b4:{ c1=l7;
if c1{ l6=2i64;
break 'b4;
 }else{ l6=3i64;
break 'b4;
 }
 }
l9=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4193,8))? as *const C4181)).f3;
l10=l6;
l8={let a0=l9;let a1=l2;let a2=l3;let a3=l4;let a4=l5;let a5=l10;let p=(rt::try_alloc(8, 64)).map_err(|e|e.at(&S4193,10))? as *mut C4180;bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f0).cast(),a0);(*p).f1=a1 as _;(*p).f2=a2 as _;bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f3).cast(),a3);rt::write_dyn(p.cast(),&raw mut (*p).f4,a4);(*p).f5=a5 as _;p as usize};
{let value=l8;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4193,11))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4181)).f4,value);}
{let value=l8;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4193,12))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4181)).f3,value);}
l11=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4193,13))? as *const C4181)).f2;
l12=l2;
{let value=l8;let object=l11;let slot=(rt::list_slot(object,l12)).map_err(|e|e.at(&S4193,14))?;bex_v2_heap::write_ptr(object as *mut u8,slot.cast(),value);}
l0=l8;
return Ok(l0);
}}
static O4194:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4194:rt::Source=rt::Source{file:"main.baml",function:"user.release",lines:&[81,81,81,82,82,82,82,83,83,83,84,85,85,85,85,85,85,86,86,85,85,82,82]};
#[inline(always)]
unsafe fn f4194(mut l1:usize,mut l2:i64)->R<usize>{unsafe{rt::observe(&O4194);rt::check_stack().map_err(|e|e.at(&S4194,0))?;
let mut l0:usize=0;
let mut l3:usize=0;
let mut l4:usize=0;
let mut l5:i64=0;
let mut l6:bool=false;
let mut l7:usize=0;
let mut l8:i64=0;
let mut l9:usize=0;
let mut l10:bool=false;
let mut l11:usize=0;
let mut l12:bool=false;
let mut l13:i64=0;
let mut l14:i64=0;
let mut c1=false;
let mut c3=false;
let mut c4=false;
l4=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4194,1))? as *const C4181)).f2;
l5=l2;
l3=*((rt::list_slot(l4,l5)).map_err(|e|e.at(&S4194,2))? as *const usize);
l7=l3;
l6=(l7==0usize);
'b8:{ c1=l6;
if c1{ l0=0usize;
break 'b8;
 }else{ l8=(*((rt::nonnull(l3)).map_err(|e|e.at(&S4194,8))? as *const C4180)).f5;
{let value=(l8 & -5i64);let object=(rt::nonnull(l3)).map_err(|e|e.at(&S4194,9))?;(&raw mut (*(object as *mut C4180)).f5).write(value as _);}
l9=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4194,10))? as *const C4181)).f4;
l11=l9;
l10=(l11==0usize);
'b6:{ c3=l10;
if c3{ break 'b6;
 }else{ l13=(*((rt::nonnull(l3)).map_err(|e|e.at(&S4194,14))? as *const C4180)).f2;
l14=(*((rt::nonnull(l9)).map_err(|e|e.at(&S4194,15))? as *const C4180)).f2;
l12=(l13 > l14);
c4=l12;
if c4{ break 'b6;
 }else{ l0=l9;
break 'b8;
 }
 }
 }
l0=l3;
break 'b8;
 }
 }
return Ok(l0);
}}
static O4195:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4195:rt::Source=rt::Source{file:"main.baml",function:"user.hold_current",lines:&[89,89,90,91,91,91,91,92,92,92,93,91,91]};
#[inline(always)]
unsafe fn f4195(mut l1:usize)->R<usize>{unsafe{rt::observe(&O4195);rt::check_stack().map_err(|e|e.at(&S4195,0))?;
let mut l0:usize=0;
let mut l2:usize=0;
let mut l3:bool=false;
let mut l4:usize=0;
let mut l5:i64=0;
let mut c1=false;
{let value=(rt::add((*((rt::nonnull(l1)).map_err(|e|e.at(&S4195,1))? as *const C4181)).f1,1i64)).map_err(|e|e.at(&S4195,1))?;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4195,1))?;(&raw mut (*(object as *mut C4181)).f1).write(value as _);}
l2=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4195,2))? as *const C4181)).f4;
l4=l2;
l3=(l4==0usize);
'b4:{ c1=l3;
if c1{ l0=0usize;
break 'b4;
 }else{ l5=(*((rt::nonnull(l2)).map_err(|e|e.at(&S4195,8))? as *const C4180)).f5;
{let value=(l5 | 4i64);let object=(rt::nonnull(l2)).map_err(|e|e.at(&S4195,9))?;(&raw mut (*(object as *mut C4180)).f5).write(value as _);}
l0=(*((rt::nonnull(l2)).map_err(|e|e.at(&S4195,10))? as *const C4180)).f0;
break 'b4;
 }
 }
return Ok(l0);
}}
static O4196:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4196:rt::Source=rt::Source{file:"main.baml",function:"user.suspend_current",lines:&[96,96,97,97,97,97,98,98,98,99,97,97]};
#[inline(always)]
unsafe fn f4196(mut l1:usize)->R<usize>{unsafe{rt::observe(&O4196);rt::check_stack().map_err(|e|e.at(&S4196,0))?;
let mut l0:usize=0;
let mut l2:usize=0;
let mut l3:bool=false;
let mut l4:usize=0;
let mut l5:i64=0;
let mut c1=false;
l2=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4196,1))? as *const C4181)).f4;
l4=l2;
l3=(l4==0usize);
'b4:{ c1=l3;
if c1{ l0=0usize;
break 'b4;
 }else{ l5=(*((rt::nonnull(l2)).map_err(|e|e.at(&S4196,7))? as *const C4180)).f5;
{let value=(l5 | 2i64);let object=(rt::nonnull(l2)).map_err(|e|e.at(&S4196,8))?;(&raw mut (*(object as *mut C4180)).f5).write(value as _);}
l0=l2;
break 'b4;
 }
 }
return Ok(l0);
}}
static O4197:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4197:rt::Source=rt::Source{file:"main.baml",function:"user.enqueue",lines:&[102,102,102,102,103,103,103,103,104,104,105,105,105,105,106,106,107,108,109,109,103,103]};
#[inline(always)]
unsafe fn f4197(mut l1:usize,mut l2:usize)->R<usize>{unsafe{rt::observe(&O4197);rt::check_stack().map_err(|e|e.at(&S4197,0))?;
let mut l0:usize=0;
let mut l3:usize=0;
let mut l4:usize=0;
let mut l5:i64=0;
let mut l6:bool=false;
let mut l7:usize=0;
let mut l8:usize=0;
let mut l9:bool=false;
let mut l10:usize=0;
let mut l11:usize=0;
let mut l12:usize=0;
let mut c1=false;
let mut c3=false;
l4=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4197,1))? as *const C4181)).f2;
l5=(*((rt::nonnull(l2)).map_err(|e|e.at(&S4197,2))? as *const C4179)).f1;
l3=*((rt::list_slot(l4,l5)).map_err(|e|e.at(&S4197,3))? as *const usize);
l7=l3;
l6=(l7==0usize);
'b7:{ c1=l6;
if c1{ l0=0usize;
break 'b7;
 }else{ l8=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4197,9))? as *const C4181)).f4;
l10=l8;
l9=(l10==0usize);
c3=l9;
if c3{ l0=0usize;
break 'b7;
 }else{ {let value=(rt::add((*((rt::nonnull(l1)).map_err(|e|e.at(&S4197,15))? as *const C4181)).f0,1i64)).map_err(|e|e.at(&S4197,15))?;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4197,15))?;(&raw mut (*(object as *mut C4181)).f0).write(value as _);}
{let value=0usize;let object=(rt::nonnull(l2)).map_err(|e|e.at(&S4197,16))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4179)).f0,value);}
{let value=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4197,17))? as *const C4181)).f5;let object=(rt::nonnull(l2)).map_err(|e|e.at(&S4197,17))?;(&raw mut (*(object as *mut C4179)).f1).write(value as _);}
l11=l3;
l12=l8;
l0=(f4191(l11,l12,l2)).map_err(|e|e.at(&S4197,19))?;
break 'b7;
 }
 }
 }
return Ok(l0);
}}
static O4198:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4198:rt::Source=rt::Source{file:"main.baml",function:"user.<(user.IdleTask as user.Task)>.run",lines:&[116,116,117,117,117,117,118,118,118,118,119,119,119,120,122,122,122,122,123,117,117]};
#[inline(always)]
unsafe fn f4198(mut l1:usize,mut l2:usize,mut l3:usize)->R<usize>{unsafe{rt::observe(&O4198);rt::check_stack().map_err(|e|e.at(&S4198,0))?;
let mut l0:usize=0;
let mut l4:bool=false;
let mut l5:i64=0;
let mut l6:bool=false;
let mut l7:i64=0;
let mut l8:i64=0;
let mut l9:i64=0;
let mut l10:i64=0;
let mut l11:i64=0;
let mut c0=false;
let mut c1=false;
'b5:{ {let value=(rt::sub((*((rt::nonnull(l1)).map_err(|e|e.at(&S4198,1))? as *const C4182)).f1,1i64)).map_err(|e|e.at(&S4198,1))?;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4198,1))?;(&raw mut (*(object as *mut C4182)).f1).write(value as _);}
l5=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4198,2))? as *const C4182)).f1;
l4=(l5 == 0i64);
c0=l4;
if c0{ l0=(f4195(l2)).map_err(|e|e.at(&S4198,5))?;
break 'b5;
 }else{ l8=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4198,7))? as *const C4182)).f0;
l7=(l8 & 1i64);
l6=(l7 == 0i64);
c1=l6;
if c1{ l9=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4198,11))? as *const C4182)).f0;
{let value=(rt::shr(l9,1i64)).map_err(|e|e.at(&S4198,12))?;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4198,12))?;(&raw mut (*(object as *mut C4182)).f0).write(value as _);}
l0=(f4194(l2,4i64)).map_err(|e|e.at(&S4198,13))?;
break 'b5;
 }else{ l11=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4198,15))? as *const C4182)).f0;
l10=(rt::shr(l11,1i64)).map_err(|e|e.at(&S4198,16))?;
{let value=(l10 ^ 53256i64);let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4198,17))?;(&raw mut (*(object as *mut C4182)).f0).write(value as _);}
l0=(f4194(l2,5i64)).map_err(|e|e.at(&S4198,18))?;
break 'b5;
 }
 }
 }
return Ok(l0);
}}
static O4199:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4199:rt::Source=rt::Source{file:"main.baml",function:"user.<(user.DeviceTask as user.Task)>.run",lines:&[131,131,131,132,132,133,133,133,133,134,134,135,137,137,138,133,133]};
#[inline(always)]
unsafe fn f4199(mut l1:usize,mut l2:usize,mut l3:usize)->R<usize>{unsafe{rt::observe(&O4199);rt::check_stack().map_err(|e|e.at(&S4199,0))?;
let mut l0:usize=0;
let mut l4:bool=false;
let mut l5:usize=0;
let mut l6:bool=false;
let mut l7:usize=0;
let mut l8:usize=0;
let mut c1=false;
let mut c4=false;
l4=(l3==0usize);
'b7:{ c1=l4;
if c1{ l5=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4199,4))? as *const C4183)).f0;
l7=l5;
l6=(l7==0usize);
c4=l6;
if c4{ l0=(f4196(l2)).map_err(|e|e.at(&S4199,8))?;
break 'b7;
 }else{ {let value=0usize;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4199,10))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4183)).f0,value);}
l8=l5;
l0=(f4197(l2,l8)).map_err(|e|e.at(&S4199,11))?;
break 'b7;
 }
 }else{ {let value=l3;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4199,13))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4183)).f0,value);}
l0=(f4195(l2)).map_err(|e|e.at(&S4199,14))?;
break 'b7;
 }
 }
return Ok(l0);
}}
static O4200:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4200:rt::Source=rt::Source{file:"main.baml",function:"user.<(user.WorkerTask as user.Task)>.run",lines:&[147,147,147,147,147,148,148,148,148,148,148,148,149,149,150,151,151,151,151,152,152,153,153,153,153,154,154,154,154,151,151,156,156,147,147]};
#[inline(always)]
unsafe fn f4200(mut l1:usize,mut l2:usize,mut l3:usize)->R<usize>{unsafe{rt::observe(&O4200);rt::check_stack().map_err(|e|e.at(&S4200,0))?;
let mut l0:usize=0;
let mut l4:bool=false;
let mut l5:bool=false;
let mut l6:i64=0;
let mut l7:i64=0;
let mut l8:bool=false;
let mut l9:i64=0;
let mut l10:bool=false;
let mut l11:i64=0;
let mut l12:usize=0;
let mut l13:i64=0;
let mut c1=false;
let mut c2=false;
let mut c6=false;
let mut c8=false;
let mut poll6=0u32;
l4=(l3==0usize);
'b13:{ c1=l4;
if c1{ l0=(f4196(l2)).map_err(|e|e.at(&S4200,4))?;
break 'b13;
 }else{ 'b5:{ l6=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4200,6))? as *const C4184)).f0;
l5=(l6 == 2i64);
c2=l5;
if c2{ {let value=3i64;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4200,9))?;(&raw mut (*(object as *mut C4184)).f0).write(value as _);}
break 'b5;
 }else{ {let value=2i64;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4200,11))?;(&raw mut (*(object as *mut C4184)).f0).write(value as _);}
break 'b5;
 }
 }
{let value=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4200,13))? as *const C4184)).f0;let object=(rt::nonnull(l3)).map_err(|e|e.at(&S4200,13))?;(&raw mut (*(object as *mut C4179)).f1).write(value as _);}
{let value=0i64;let object=(rt::nonnull(l3)).map_err(|e|e.at(&S4200,14))?;(&raw mut (*(object as *mut C4179)).f3).write(value as _);}
l7=0i64;
'l6:loop{ poll6=poll6.wrapping_add(1);if poll6&15==0{(rt::poll()).map_err(|e|e.at(&S4200,16))?;}
l9=l7;
l8=(l9 < 4i64);
c6=l8;
if c6{ 'b10:{ {let value=(rt::add((*((rt::nonnull(l1)).map_err(|e|e.at(&S4200,20))? as *const C4184)).f1,1i64)).map_err(|e|e.at(&S4200,20))?;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4200,20))?;(&raw mut (*(object as *mut C4184)).f1).write(value as _);}
l11=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4200,21))? as *const C4184)).f1;
l10=(l11 > 26i64);
c8=l10;
if c8{ {let value=1i64;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4200,24))?;(&raw mut (*(object as *mut C4184)).f1).write(value as _);}
break 'b10;
 }else{ break 'b10;
 }
 }
l12=(*((rt::nonnull(l3)).map_err(|e|e.at(&S4200,26))? as *const C4179)).f4;
l13=l7;
{let value=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4200,28))? as *const C4184)).f1;let object=l12;let slot=(rt::list_slot(object,l13)).map_err(|e|e.at(&S4200,28))?;slot.cast::<i64>().write(value);}
l7=(rt::add(l7,1i64)).map_err(|e|e.at(&S4200,30))?;
continue 'l6;
 }else{ break 'l6;
 }
 }
l0=(f4197(l2,l3)).map_err(|e|e.at(&S4200,32))?;
break 'b13;
 }
 }
return Ok(l0);
}}
static O4201:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4201:rt::Source=rt::Source{file:"main.baml",function:"user.<(user.HandlerTask as user.Task)>.run",lines:&[165,165,165,165,166,166,166,166,166,166,166,166,167,167,167,167,167,169,169,170,170,170,171,171,172,173,173,174,174,174,175,175,176,176,176,177,178,181,181,182,185,185,178,178]};
#[inline(always)]
unsafe fn f4201(mut l1:usize,mut l2:usize,mut l3:usize)->R<usize>{unsafe{rt::observe(&O4201);rt::check_stack().map_err(|e|e.at(&S4201,0))?;
let mut l0:usize=0;
let mut l4:bool=false;
let mut l5:bool=false;
let mut l6:bool=false;
let mut l7:i64=0;
let mut l8:usize=0;
let mut l9:usize=0;
let mut l10:usize=0;
let mut l11:usize=0;
let mut l12:usize=0;
let mut l13:bool=false;
let mut l14:usize=0;
let mut l15:bool=false;
let mut l16:i64=0;
let mut l17:bool=false;
let mut l18:i64=0;
let mut l19:usize=0;
let mut l20:bool=false;
let mut l21:usize=0;
let mut l22:bool=false;
let mut l23:usize=0;
let mut l24:i64=0;
let mut l25:i64=0;
let mut l26:usize=0;
let mut l27:usize=0;
let mut c1=false;
let mut c2=false;
let mut c8=false;
let mut c9=false;
let mut c12=false;
l5=(l3==0usize);
'b7:{ l4=!(l5);
c1=l4;
if c1{ l7=(*((rt::nonnull(l3)).map_err(|e|e.at(&S4201,5))? as *const C4179)).f2;
l6=(l7 == 1i64);
c2=l6;
if c2{ l8=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4201,8))? as *const C4185)).f0;
l9=(f4188(l3,l8)).map_err(|e|e.at(&S4201,9))?;
{let value=l9;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4201,11))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4185)).f0,value);}
break 'b7;
 }else{ l10=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4201,13))? as *const C4185)).f1;
l11=(f4188(l3,l10)).map_err(|e|e.at(&S4201,14))?;
{let value=l11;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4201,16))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4185)).f1,value);}
break 'b7;
 }
 }else{ break 'b7;
 }
 }
l12=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4201,18))? as *const C4185)).f0;
l14=l12;
l15=(l14==0usize);
'b15:{ 'b13:{ l13=!(l15);
c8=l13;
if c8{ l16=(*((rt::nonnull(l12)).map_err(|e|e.at(&S4201,23))? as *const C4179)).f3;
l18=l16;
l17=(l18 < 4i64);
c9=l17;
if c9{ l19=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4201,26))? as *const C4185)).f1;
l21=l19;
l22=(l21==0usize);
l20=!(l22);
c12=l20;
if c12{ {let value=(*((rt::nonnull(l19)).map_err(|e|e.at(&S4201,31))? as *const C4179)).f0;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4201,31))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4185)).f1,value);}
l23=(*((rt::nonnull(l12)).map_err(|e|e.at(&S4201,32))? as *const C4179)).f4;
l24=l16;
{let value=*((rt::list_slot(l23,l24)).map_err(|e|e.at(&S4201,34))? as *const i64);let object=(rt::nonnull(l19)).map_err(|e|e.at(&S4201,34))?;(&raw mut (*(object as *mut C4179)).f3).write(value as _);}
l25=l16;
{let value=(rt::add(l25,1i64)).map_err(|e|e.at(&S4201,35))?;let object=(rt::nonnull(l12)).map_err(|e|e.at(&S4201,35))?;(&raw mut (*(object as *mut C4179)).f3).write(value as _);}
l26=l19;
l0=(f4197(l2,l26)).map_err(|e|e.at(&S4201,36))?;
break 'b15;
 }else{ break 'b13;
 }
 }else{ {let value=(*((rt::nonnull(l12)).map_err(|e|e.at(&S4201,38))? as *const C4179)).f0;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4201,38))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4185)).f0,value);}
l27=l12;
l0=(f4197(l2,l27)).map_err(|e|e.at(&S4201,39))?;
break 'b15;
 }
 }else{ break 'b13;
 }
 }
l0=(f4196(l2)).map_err(|e|e.at(&S4201,41))?;
break 'b15;
 }
return Ok(l0);
}}
static O4202:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4202:rt::Source=rt::Source{file:"main.baml",function:"user.richards",lines:&[190,190,190,191,191,192,192,193,194,194,194,195,195,195,195,195,196,196,197,197,197,198,198,198,199,199,199,199,199,200,200,201,201,201,202,202,202,203,203,203,203,203,204,204,204,204,205,205,205,205,206,206,208,208,209,209,217,217,217,217,217,217,210,210,210,210,211,211,213,213,214,214,214,214]};
#[inline(always)]
unsafe fn f4202()->R<i64>{unsafe{rt::observe(&O4202);rt::check_stack().map_err(|e|e.at(&S4202,0))?;
let mut l0:i64=0;
let mut l1:usize=0;
let mut l2:usize=0;
let mut l3:usize=0;
let mut l4:usize=0;
let mut l5:usize=0;
let mut l6:usize=0;
let mut l7:usize=0;
let mut l8:usize=0;
let mut l9:usize=0;
let mut l10:usize=0;
let mut l11:usize=0;
let mut l12:usize=0;
let mut l13:usize=0;
let mut l14:usize=0;
let mut l15:usize=0;
let mut l16:usize=0;
let mut l17:usize=0;
let mut l18:usize=0;
let mut l19:usize=0;
let mut l20:usize=0;
let mut l21:usize=0;
let mut l22:usize=0;
let mut l23:usize=0;
let mut l24:usize=0;
let mut l25:usize=0;
let mut l26:usize=0;
let mut l27:usize=0;
let mut l28:usize=0;
let mut l29:usize=0;
let mut l30:usize=0;
let mut l31:bool=false;
let mut l32:usize=0;
let mut l33:bool=false;
let mut l34:usize=0;
let mut l35:usize=0;
let mut l36:usize=0;
let mut l37:usize=0;
let mut l38:i64=0;
let mut l39:i64=0;
let mut l40:i64=0;
let mut c16=false;
let mut c18=false;
let mut poll15=0u32;
l2={let a0=0usize;let a1=0usize;let a2=0usize;let a3=0usize;let a4=0usize;let a5=0usize;let p=(rt::try_alloc(15,64)).map_err(|e|e.at(&S4202,1))?;p.add(8).cast::<usize>().write(6);bex_v2_heap::write_ptr(p,p.add(16).cast(),a0);bex_v2_heap::write_ptr(p,p.add(24).cast(),a1);bex_v2_heap::write_ptr(p,p.add(32).cast(),a2);bex_v2_heap::write_ptr(p,p.add(40).cast(),a3);bex_v2_heap::write_ptr(p,p.add(48).cast(),a4);bex_v2_heap::write_ptr(p,p.add(56).cast(),a5);p as usize};
l1={let a0=0i64;let a1=0i64;let a2=l2;let a3=0usize;let a4=0usize;let a5=0i64;let p=(rt::try_alloc(9, 56)).map_err(|e|e.at(&S4202,2))? as *mut C4181;(*p).f0=a0 as _;(*p).f1=a1 as _;bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f2).cast(),a2);bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f3).cast(),a3);bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f4).cast(),a4);(*p).f5=a5 as _;p as usize};
l4=l1;
l5={let a0=1i64;let a1=1000i64;let p=(rt::try_alloc(10, 24)).map_err(|e|e.at(&S4202,3))? as *mut C4182;(*p).f0=a0 as _;(*p).f1=a1 as _;p as usize};
l3=(f4193(l4,0i64,0i64,0usize,rt::Dyn{tag:13,payload:(l5) as u64})).map_err(|e|e.at(&S4202,4))?;
{let value=0i64;let object=(rt::nonnull(l3)).map_err(|e|e.at(&S4202,6))?;(&raw mut (*(object as *mut C4180)).f5).write(value as _);}
l6=(f4187(0usize,1i64,1i64)).map_err(|e|e.at(&S4202,7))?;
l7=l6;
l6=(f4187(l7,1i64,1i64)).map_err(|e|e.at(&S4202,10))?;
l9=l1;
l10=l6;
l11={let a0=2i64;let a1=0i64;let p=(rt::try_alloc(12, 24)).map_err(|e|e.at(&S4202,14))? as *mut C4184;(*p).f0=a0 as _;(*p).f1=a1 as _;p as usize};
l8=(f4193(l9,1i64,1000i64,l10,rt::Dyn{tag:15,payload:(l11) as u64})).map_err(|e|e.at(&S4202,15))?;
l6=(f4187(0usize,4i64,0i64)).map_err(|e|e.at(&S4202,17))?;
l12=l6;
l6=(f4187(l12,4i64,0i64)).map_err(|e|e.at(&S4202,20))?;
l13=l6;
l6=(f4187(l13,4i64,0i64)).map_err(|e|e.at(&S4202,23))?;
l15=l1;
l16=l6;
l17={let a0=0usize;let a1=0usize;let p=(rt::try_alloc(13, 24)).map_err(|e|e.at(&S4202,27))? as *mut C4185;bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f0).cast(),a0);bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f1).cast(),a1);p as usize};
l14=(f4193(l15,2i64,2000i64,l16,rt::Dyn{tag:16,payload:(l17) as u64})).map_err(|e|e.at(&S4202,28))?;
l6=(f4187(0usize,5i64,0i64)).map_err(|e|e.at(&S4202,30))?;
l18=l6;
l6=(f4187(l18,5i64,0i64)).map_err(|e|e.at(&S4202,33))?;
l19=l6;
l6=(f4187(l19,5i64,0i64)).map_err(|e|e.at(&S4202,36))?;
l21=l1;
l22=l6;
l23={let a0=0usize;let a1=0usize;let p=(rt::try_alloc(13, 24)).map_err(|e|e.at(&S4202,40))? as *mut C4185;bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f0).cast(),a0);bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f1).cast(),a1);p as usize};
l20=(f4193(l21,3i64,3000i64,l22,rt::Dyn{tag:16,payload:(l23) as u64})).map_err(|e|e.at(&S4202,41))?;
l25=l1;
l26={let a0=0usize;let p=(rt::try_alloc(11, 16)).map_err(|e|e.at(&S4202,44))? as *mut C4183;bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f0).cast(),a0);p as usize};
l24=(f4193(l25,4i64,4000i64,0usize,rt::Dyn{tag:14,payload:(l26) as u64})).map_err(|e|e.at(&S4202,45))?;
l28=l1;
l29={let a0=0usize;let p=(rt::try_alloc(11, 16)).map_err(|e|e.at(&S4202,48))? as *mut C4183;bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f0).cast(),a0);p as usize};
l27=(f4193(l28,5i64,5000i64,0usize,rt::Dyn{tag:14,payload:(l29) as u64})).map_err(|e|e.at(&S4202,49))?;
{let value=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4202,51))? as *const C4181)).f3;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4202,51))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4181)).f4,value);}
'l15:loop{ poll15=poll15.wrapping_add(1);if poll15&15==0{(rt::poll()).map_err(|e|e.at(&S4202,52))?;}
l30=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4202,53))? as *const C4181)).f4;
l32=l30;
l31=(l32==0usize);
c16=l31;
if c16{ l39=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4202,56))? as *const C4181)).f0;
l38=(rt::mul(l39,10000i64)).map_err(|e|e.at(&S4202,57))?;
l40=(*((rt::nonnull(l1)).map_err(|e|e.at(&S4202,58))? as *const C4181)).f1;
l0=(rt::add(l38,l40)).map_err(|e|e.at(&S4202,59))?;
return Ok(l0);
 }else{ l34=l30;
l33=(f4192(l34)).map_err(|e|e.at(&S4202,64))?;
c18=l33;
if c18{ {let value=(*((rt::nonnull(l30)).map_err(|e|e.at(&S4202,67))? as *const C4180)).f0;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4202,67))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4181)).f4,value);}
continue 'l15;
 }else{ {let value=(*((rt::nonnull(l30)).map_err(|e|e.at(&S4202,69))? as *const C4180)).f1;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4202,69))?;(&raw mut (*(object as *mut C4181)).f5).write(value as _);}
l35=l30;
l36=l1;
l37=(f4190(l35,l36)).map_err(|e|e.at(&S4202,71))?;
{let value=l37;let object=(rt::nonnull(l1)).map_err(|e|e.at(&S4202,73))?;bex_v2_heap::write_ptr(object as *mut u8,&raw mut (*(object as *mut C4181)).f4,value);}
continue 'l15;
 }
 }
 }
}}
static O4203:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);
static S4203:rt::Source=rt::Source{file:"main.baml",function:"user.main",lines:&[220,220,221,221,221,221,221,221,221,221,221,221,222,222,222]};
#[inline(always)]
unsafe fn f4203()->R<i64>{unsafe{rt::observe(&O4203);rt::check_stack().map_err(|e|e.at(&S4203,0))?;
let mut l0:i64=0;
let mut l1:i64=0;
let mut l2:i64=0;
let mut l3:bool=false;
let mut l4:i64=0;
let mut l5:i64=0;
let mut c1=false;
let mut poll1=0u32;
l1=0i64;
l2=0i64;
'l1:loop{ poll1=poll1.wrapping_add(1);if poll1&15==0{(rt::poll()).map_err(|e|e.at(&S4203,3))?;}
l4=l2;
l3=(l4 < 50i64);
c1=l3;
if c1{ l5=(f4202()).map_err(|e|e.at(&S4203,7))?;
l1=(rt::add(l1,l5)).map_err(|e|e.at(&S4203,9))?;
l2=(rt::add(l2,1i64)).map_err(|e|e.at(&S4203,11))?;
continue 'l1;
 }else{ break 'l1;
 }
 }
l0=l1;
return Ok(l0);
}}
fn entry()->R<i64>{unsafe{f4203()}}
fn main()->std::process::ExitCode{O4187.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);O4188.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);O4190.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);O4191.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);O4192.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);O4193.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);O4194.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);O4195.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);O4196.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);O4197.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);O4198.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);O4199.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);O4200.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);O4201.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);O4202.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);O4203.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);rt::driver(DESCRIPTORS,entry)}
