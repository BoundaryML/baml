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
