function RtNativeMath()->int {
                let a=bigint.parse("123456789012345678901234567890");
                let b=(a*9n+1n)/3n;
                let width=bigint._random_byte_count(0n,256n);
                let sample=bigint._random_in_range(b"x",5n,6n);
                if ((b%7n)==0n && (16n).isqrt()==4n && (-9n).abs()==9n && (2n).pow(10n)==1024n && (1000n).ilog(10n)==3n && sample==5n && width==1) { 7 } else {9}
            }

function RtNativeBytes()->int {
                let a=uint8array.from_hex("ff00017f"); let alias=a;
                a.push(256); a.sort(); a[-1]=258;
                let reversed=a.reverse();
                let b=uint8array.from_base64(a.to_base64()).concat(b"x");
                let c=uint8array.from_array(a.to_array());
                let last=b.pop(); let cut=b.slice(-2,99);
                let zero=uint8array.zeroes(2);
                if (alias[0]==0 && (a.at(-1) ?? -1)==2 && reversed[0]==2 && c.length()==5 && (last ?? -1)==120 && cut.length()==2 && (zero.at(0) ?? -1)==0 && a.includes(127) && (a.index_of(127) ?? -1)==3 && a.to_hex().char_count()==10 && b"hi"._to_string_impl().char_count()==2) {7}else{9}
            }

function RtNativeClock() -> int {
    let info=baml.sys.host_info();
    let timezone=baml.time.system_timezone();
    let now=baml.time.Instant.now();
    baml.sys.sleep(baml.time.Duration.from_milliseconds(-1));
    baml.sys.sleep(baml.time.Duration.from_milliseconds(1));
    baml.sys.collect_garbage();
    let offset=baml.time._tz_offset_at("Etc/UTC", -1n) ?? -1;
    let missing=baml.time._tz_offset_at("Not/A/Timezone", 0n);
    let epoch=baml.time._tz_to_instant("Etc/UTC",0n,"compatible") ?? -1n;
    if (info.cpu_count >= 0 && info.platform.char_count()>0 && info.architecture.char_count()>0 && timezone.char_count()>0 && baml.sys.host_target().char_count()>0 && now._nanoseconds>0n && offset==0 && missing==null && epoch==0n) {7}else{9}
}

function SameNativeBytes(a:uint8array,b:uint8array)->bool {
    if (a.length()!=b.length()) {return false;}
    let i=0;
    while(i<a.length()) {if(a[i]!=b[i]){return false;} i+=1;}
    true
}
function DrawNativeRandom(rng:baml.random.Rng)->int {
    let bytes=rng.random(9);
    let n=rng.random_int();
    if (bytes.length()==9 && n>=int.min_value() && n<=int.max_value()) {1}else{0}
}
function RtNativeRandom()->int {
    let seed=uint8array.from_hex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
    let x=baml.random.Xoshiro256PlusPlus._new(seed);
    let alias=x;
    let twin=baml.random.Xoshiro256PlusPlus._new(seed);
    let c=baml.random.ChaCha20._new(seed);
    let ct=baml.random.ChaCha20._new(seed);
    let stream=x.random(8);
    let other=twin.random(8);
    let xi=alias.random_int(); let ti=twin.random_int();
    let cs=c.random(11); let ts=ct.random(11);
    let ci=c.random_int(); let cti=ct.random_int();
    let a=DrawNativeRandom(x);
    let b=DrawNativeRandom(c);
    let s=DrawNativeRandom(baml.random.SystemRandom.get());
    let default_seed=baml.random.ChaCha20._new(baml.random.SystemRandom.get().random(32));
    baml.io.println(stream.to_hex());
    baml.io.println(cs.to_hex());
    baml.io.println(`${xi}:${ci}`);
    if (SameNativeBytes(stream,other) && xi==ti && SameNativeBytes(cs,ts) && ci==cti && a+b+s==3 && default_seed.random(0).length()==0) {7}else{9}
}
