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

function RtNativeFiles()->int {
    let home=baml.env.get("BAML_HOME") ?? ".";
    let dir=baml.path.join([home,"native-files"]);
    let path=baml.path.join([dir,"data.bin"]);
    baml.fs.mkdir(dir,baml.fs.MkdirOptions {recursive:true});
    baml.fs.write_bytes(path,b"old");
    baml.fs.write_atomic(path,b"new");
    let f=baml.fs.open(path,"w+");
    let alias=f;
    let writer=f.as<baml.io.Write>;
    let reader=f.as<baml.io.Read>;
    let written=writer.write_some(b"hello");
    writer.flush();
    let pos=f.seek_from("start",0);
    let prefix=reader.read(2) ?? b"";
    let rest=reader.bytes();
    let end=reader.read(2);
    let zero=reader.read(0) ?? b"x";
    f.close(); alias.close();
    let closed={reader.read(1);false} catch(e) {baml.errors.Io {message}=>message.char_count()>0};
    baml.fs.chmod(path,384);
    let link=baml.path.join([dir,"link.bin"]);
    baml.fs.symlink("data.bin",link);
    let meta=baml.fs.metadata(link);
    let entries=baml.fs.read_dir(dir);
    let parent=baml.path.parent(path) ?? "";
    let absolute=baml.path._absolute(path,null);
    let lock=baml.fs.lock(baml.path.join([dir,"guard"]),1000);
    lock.close();lock.close();
    baml.io.println(prefix.to_hex());baml.io.println(rest.to_hex());
    if(written==5 && pos==0 && prefix.length()==2 && rest.length()==3 && end==null && zero.length()==0 && closed && meta.is_file && !meta.is_dir && entries[0].name.char_count()>0 && parent.char_count()>0 && baml.path.is_absolute(absolute)) {7}else{9}
}

function RtNativeProcess()->int {
    let options=baml.sys._ProcessOptions {
        cwd:null,env:{"RT_NATIVE_ONLY":"owned","RT_NATIVE_REMOVED":null},inherit_env:false,
        timeout:baml.time.Duration.from_seconds(10),input:b"input\n",
        stdin:"pipe",stdout:"pipe",stderr:"pipe",detached:false,
    };
    let out=baml.sys._run("/bin/sh",["-c",`read line; printf '%s:%s' "$line" "$RT_NATIVE_ONLY"; printf err >&2; exit 7`],options);
    let shell=baml.sys._shell("printf shell",baml.sys._ProcessOptions {
        cwd:null,env:null,inherit_env:true,timeout:null,input:null,
        stdin:"ignore",stdout:"pipe",stderr:"ignore",detached:false,
    });
    baml.io.println(out.stdout.to_hex());baml.io.println(out.stderr.to_hex());
    baml.io.println(shell.stdout.to_hex());
    if(out.exit_code==7 && out.signal==null && out.stdout.length()==11 && out.stderr.length()==3 && shell.ok() && shell.stdout.length()==5) {7}else{9}
}

function RtNativeCalendar()->int {
    let instant=baml.time.Instant.parse("1970-01-01T01:00:00+01:00");
    let rfc=baml.time.Instant.parse("Thu, 01 Jan 1970 00:00:00 +0000");
    let date=baml.time.PlainDate.from_components(2024,2,29);
    let parsed_date=baml.time.PlainDate.parse("2024-02-29");
    let clock=baml.time.PlainTime._from_components(23,59,58,123,456,789);
    let parsed_clock=baml.time.PlainTime.parse("23:59:58.123456789");
    let datetime=baml.time.PlainDateTime._from_components(2024,2,29,23,59,58,123,456,789);
    let parsed_datetime=baml.time.PlainDateTime.parse("2024-02-29T23:59:58.123456789");
    let combined=date._to_plain_datetime(clock._nanoseconds);
    let offset=baml.time.TimeZoneOffset.from_duration(baml.time.Duration.from_hours(-7));
    let zoned=baml.time.ZonedDateTime.parse("1970-01-01T01:00:00+01:00[!Etc/UTC]");
    let fixed=baml.time.ZonedDateTime.parse("1970-01-01T01:00:00+01:00");
    let before=baml.time.PlainDateTime {_nanoseconds:-1n};
    baml.io.println(instant._to_string_impl());
    baml.io.println(date._to_string_impl());
    baml.io.println(clock._to_string_impl());
    baml.io.println(datetime._to_string_impl());
    baml.io.println(baml.time._format_zoned(0n,3600000000000,null));
    baml.io.println(baml.time._format_zoned(0n,0,"Etc/UTC"));
    let bad_date={baml.time.PlainDate.from_components(2023,2,29);false} catch(e) {baml.errors.InvalidArgument {message}=>{baml.io.println(message);true}};
    let bad_time={baml.time.PlainTime.parse("25:00:00");false} catch(e) {baml.errors.ParseError {message}=>{baml.io.println(message);true}};
    let bad_offset={baml.time.TimeZoneOffset.from_duration(baml.time.Duration.from_hours(25));false} catch(e) {baml.errors.InvalidArgument {message}=>{baml.io.println(message);true}};
    if (instant._nanoseconds==0n && rfc._nanoseconds==0n && date._days==parsed_date._days && date.year()==2024 && date.month()==2 && date.day()==29 && clock._nanoseconds==parsed_clock._nanoseconds && combined._nanoseconds==datetime._nanoseconds && datetime._nanoseconds==parsed_datetime._nanoseconds && datetime.year()==2024 && datetime.month()==2 && datetime.day()==29 && datetime.hour()==23 && datetime.minute()==59 && datetime.second()==58 && datetime.millisecond()==123 && datetime.to_plain_date()._days==date._days && datetime.to_plain_time()._nanoseconds==clock._nanoseconds && before.to_plain_date()._days == -1 && before.to_plain_time()._nanoseconds==86399999999999 && offset._nanoseconds == -25200000000000 && zoned._nanoseconds==0n && zoned._offset_ns==null && zoned._iana=="Etc/UTC" && (fixed._offset_ns ?? -1)==3600000000000 && fixed._iana==null && bad_date && bad_time && bad_offset) {7}else{9}
}
