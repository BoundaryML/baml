class RuntimeListBox {
    values: int[],
    rows: int[][],
    flags: bool[],
    points: float[],
}

class RuntimeListItem {
    value: int,
}

function RuntimeLists() -> int {
    let xs: int[] = [];
    for (let i = 0; i < 129; i += 1) {
        xs.push(i);
    }
    let alias = xs;
    xs[-1] = 900;
    let copy = xs.slice(-3, 200).reverse();
    let rows: int[][] = [xs, copy];
    rows[1].push(700);
    let flags = [true, false];
    flags[1] = true;
    let points = [1.5, 2.5];
    points.push(3.5);
    points[0] = -0.5;
    let box = RuntimeListBox { values: alias, rows: rows, flags: flags, points: points };
    let items: RuntimeListItem[] = [RuntimeListItem { value: 42 }];
    items.push(RuntimeListItem { value: 43 });
    items[-1].value = 44;
    let tagged: (int | string | bool | null)[] = [1, "two", true, null];
    tagged.push("five");
    tagged[0] = false;
    let strings = ["a", "b"];
    strings[0] = "c";
    strings.unshift("d");
    strings.remove_at(-1);
    let joined = strings.join(":");
    let total = 0;
    for (let row in box.rows) {
        for (let value in row) {
            total += value;
        }
    }
    if (box.flags[1] && box.points[0] < 0.0 && joined == "d:c") {
        total + items[1].value + tagged.length() + box.values[-1]
    } else {
        -1
    }
}

function RuntimeListBounds() -> int {
    let xs = [10, 20];
    xs[2]
}

function RuntimeListStoreBounds() -> int {
    let xs = ["a"];
    xs[-2] = "bad";
    0
}

function RuntimeListJson() -> int {
    let rows = baml.json.decode<int[][]>("[[1,2],[3,4]]");
    let result = 0;
    for (let row in rows) {
        for (let value in row) {
            result += value;
        }
    }
    result
}

function RuntimeListJsonField() -> int {
    let box = baml.json.decode<RuntimeListBox>(
        "{\"values\":[1,2],\"rows\":[[3]],\"flags\":[true],\"points\":[1.5]}",
    );
    box.values[1] + box.rows[0][0]
}

function RuntimeListJsonError() -> int {
    let rows = baml.json.decode<int[][]>("[[1],[2,\"bad\"]]");
    rows.length()
}

function RuntimeListJsonOptional() -> int {
    let values = baml.json.decode<(int | null)[]>("[1,null,2]");
    let encoded = baml.json.stringify(values);
    if (encoded == "[1,null,2]") {
        values.length()
    } else {
        -1
    }
}

function RuntimeListJsonOptionalError() -> int {
    let values = baml.json.decode<(int | null)[]>("[1,\"bad\"]");
    values.length()
}

function RuntimeListBuiltins() -> int {
    let values = baml.Array.filled(4, 7);
    values.insert(3, -1);
    values.splice(1, 2, values);
    values.unshift(2);
    values.pop();
    values.shift();
    let text = values.join(":");
    if (values.includes(3) && values.index_of(3) != null && values.last_index_of(7) != null) {
        text.length() + values.length()
    } else {
        -1
    }
}

function RuntimeListInsertError() -> int {
    let values = [1, 2];
    values.insert(3, -3)
}

function RuntimeListMaps() -> int {
    let first: map<string, int> = {};
    first.set("x", 42);
    let maps: map<string, int>[] = [first];
    maps.push(first);
    let nested = [maps];
    nested[0][1].set("y", 3);
    let total = 0;
    for (let m in nested[0]) {
        for (let v in m.values()) {
            total += v;
        }
    }
    let float_map: map<int, float> = {1: 2.5, 2: 3.5};
    let generic = [float_map];
    generic.push(float_map);
    let floats = generic[1].values();
    floats.push(4.5);
    let bool_map: map<string, bool> = {"yes": true, "no": false};
    let bools = bool_map.values();
    let deep: map<string, map<int, float>> = {"nested": float_map};
    let inner = deep.values();
    if (floats[0] == 2.5 && bools[0] && inner[0][2] == 3.5) { total } else { -1 }
}

class RuntimeListNode {
    value: int,
    children: RuntimeListNode[]?,
}

function RuntimeListRecursiveJson() -> int {
    let nodes = baml.json.decode<RuntimeListNode[]>(
        "[{\"value\":1,\"children\":[{\"value\":2,\"children\":null}]}]",
    );
    let text = baml.json.stringify(nodes);
    if (text == "[{\"value\":1,\"children\":[{\"value\":2,\"children\":null}]}]") {
        nodes[0].value
    } else {
        -1
    }
}

function RuntimeListLiteralJson() -> int {
    let values = baml.json.decode<(1 | true | "yes")[]>("[1,true,\"yes\"]");
    values.length()
}

function RuntimeListLiteralJsonError() -> int {
    let values = baml.json.decode<(1 | true | "yes")[]>("[2]");
    values.length()
}

function RuntimeListJsonFieldError() -> int {
    let value = baml.json.decode<RuntimeListBox>(
        "{\"values\":[1,\"bad\"],\"rows\":[],\"flags\":[],\"points\":[]}",
    );
    value.values.length()
}

function RuntimeListReductions() -> int {
    let floats = [3.0, 1.0, 4.0, 2.0];
    let sorted = baml._rust_sort(floats);
    if (baml._sum_float(floats) == 10.0 && baml._median_float(floats) == 2.5 && sorted[0] == 1.0) {
        baml._sum_int([1, 2, 3])
    } else {
        -1
    }
}

function RuntimeListReductionOverflow() -> int {
    baml._sum_int([4611686018427387903, 1, -1])
}

function RuntimeListMedianError() -> int {
    let floats: float[] = [];
    let value = baml._median_float(floats);
    0
}

function RuntimeListStructuralJoin() -> int {
    let rows = [[1, 2], [3]];
    let classes = [RuntimeListItem { value: 42 }];
    let first: map<string, int> = {};
    first.set("k", 5);
    let maps = [first];
    let optional: string?[] = ["a", null, "b"];
    if (
        rows.join(";") == "[1, 2];[3]"
            && classes.join(",") == "RuntimeListItem { value: 42 }"
            && maps.join(" ") == "{\"k\": 5}"
            && optional.join(":") == "a:null:b"
    ) {
        1
    } else {
        -1
    }
}

function RuntimeListFloatSpecials() -> int {
    let nan = 0.0 / 0.0;
    // Obtain zero at runtime: the VM constant pool coalesces signed-zero
    // literals, which is covered separately below.
    let zero = [0.0][0];
    let xs = [nan, 2.0, -zero, zero, -1.0];
    let sorted = baml._rust_sort(xs);
    if (
        xs.includes(nan)
            && sorted[0] == -1.0
            && sorted[-1] == nan
            && sorted.join(",") == "-1.0,-0.0,0.0,2.0,NaN"
    ) {
        1
    } else {
        -1
    }
}

function RuntimeListJsonNonFinite() -> int {
    let values = [1.0, 1.0 / 0.0];
    let text = baml.json.stringify(values);
    0
}

function NegativeListConstants() -> string {
    [-0.0, 0.0].join(",")
}
function RuntimeListFloatConstants() -> int {
    let positive_first = [0.0, -0.0].join(",");
    if (positive_first == "0.0,0.0" && NegativeListConstants() == "-0.0,-0.0") { 1 } else { -1 }
}

function RuntimeListStringEntry() -> string {
    let values = ["λ", "line\nquote\"", "😀"];
    values.push("tail");
    values.join("|")
}

function RuntimeListScalarBoundaries() -> int {
    let values: (int|float)[] = [1,2.5,-3.75];
    let total=0;
    for(let value in values) {
        total += match(value) { let i:int => i, let f:float => f.itrunc() };
    }
    let text=string.from_code_points([955,128512,10,34]);
    if(total==0 && text=="λ😀\n\"" && !text.is_ascii() && "ascii".is_ascii() && 9.0.sqrt()==3.0 && (-1.25).floor()==-2.0) {1} else {-1}
}
function RuntimeListInvalidCodePoint() -> string { string.from_code_points([65,55296]) }
function RuntimeListFloatTruncationError() -> int { (0.0/0.0).itrunc() }

class RuntimeListValidationError { message:string, offset:int, scalar:float, ok:bool }
function RuntimeListValidationThrow() -> int { throw RuntimeListValidationError {message:"quoted\"|λ", offset:2, scalar:1.5, ok:true} }

function RuntimeListCallbackIncrement(x:int) -> int { x + 1 }
function RuntimeListCallbacks() -> int {
    let xs=[1,2,3];
    let mapped=xs.map((x:int)->int { xs.push(9); x*2 });
    let selected=mapped.filter((x:int)->bool {x>2});
    let all=selected.every((x:int)->bool {x%2==0});
    let any=selected.some((x:int)->bool {x==6});
    let first=selected.find((x:int)->bool{x>3}) ?? 0;
    let last=selected.find_last((x:int)->bool{x>3}) ?? 0;
    let idx=selected.find_index((x:int)->bool{x==6}) ?? -1;
    let last_idx=selected.find_last_index((x:int)->bool{x>0}) ?? -1;
    let total=selected.reduce((a:int,x:int)->int{a+x},0);
    let flat=selected.flat_map((x:int)->int[]{[x,-x]});
    let filtered=selected.filter_map((x:int)->int? {if(x>4){x}else{null}});
    let generated=baml.Array.generate(4,(i:int)->int{i*i});
    let named=generated.map(RuntimeListCallbackIncrement);
    let seen:int[]=[];
    named.for_each((x:int)->void { seen.push(x); });
    let floats=[1.5,2.5].map((x:float)->float{x*2.0});
    let texts=selected.map((x:int)->string{x.to_string()});
    if(xs.length()==6 && mapped.length()==3 && all && any && first==4 && last==6 && idx==1 && last_idx==1 && total==10 && flat.join(",")=="4,-4,6,-6" && filtered[0]==6 && seen.join(",")=="1,2,5,10" && floats[1]==5.0 && texts.join("|")=="4|6") {1}else{-1}
}
function RuntimeListPublicSort() -> int {
    let ints=[3,1,2,1]; let alias=ints; ints.sort();
    let floats=[2.5,-1.5,0.0]; floats.sort();
    let strings=["z","a","ab"]; strings.sort();
    let bools=[true,false,true]; bools.sort();
    let reverse=[1,3,2]; reverse.sort_by((a:int,b:int)->baml.ops.Ordering {b.cmp(a)});
    if(alias.join(",")=="1,1,2,3" && floats[0]==-1.5 && strings.join(",")=="a,ab,z" && !bools[0] && reverse.join(",")=="3,2,1") {1}else{-1}
}
class RuntimeListOrdered {
    key:int
    order:int
    implements baml.ops.Equals { function eq(self,other:Self)->bool throws never { self.key==other.key } }
    implements baml.ops.Compare { function cmp(self,other:Self)->baml.ops.Ordering throws never { self.key.cmp(other.key) } }
}
function RuntimeListSortClasses() -> int {
    let xs=[RuntimeListOrdered{key:2,order:1},RuntimeListOrdered{key:1,order:2},RuntimeListOrdered{key:2,order:3}];
    xs.sort();
    let keyed=xs.reverse();
    let calls:int[]=[];
    keyed.sort_by_key((x:RuntimeListOrdered)->int {calls.push(x.order);x.key});
    if(xs[0].order==2 && xs[1].order==1 && xs[2].order==3 && keyed[0].order==2 && keyed[1].order==3 && keyed[2].order==1 && calls.join(",")=="3,1,2") {1}else{-1}
}
class RuntimeListCallbackError { value:int }
function RuntimeListCallbackThrow() -> int {
    let xs=[1,2,3];
    xs.map((x:int)->int throws RuntimeListCallbackError {if(x==2){throw RuntimeListCallbackError{value:x};}x}).length()
}
function RuntimeListSortThrow() -> int {
    let xs=[3,2,1];
    xs.sort_by((a:int,b:int)->baml.ops.Ordering throws RuntimeListCallbackError {throw RuntimeListCallbackError{value:a+b};}).length()
}

function RuntimeListTaggedSort() -> int {
    let xs:(int|float)[]=[3,1.5,-2,0.25];
    baml._rust_sort(xs);
    if(xs.join(",")=="-2,0.25,1.5,3"){1}else{-1}
}
function RuntimeListSortMixedError() -> int { let xs:(int|string)[]=[1,"a"]; baml._rust_sort(xs); 0 }
function RuntimeListSortNullError() -> int { let xs:int?[]=[1,null]; baml._rust_sort(xs); 0 }
function RuntimeListSortBoolError() -> int { baml._rust_sort([true,false]); 0 }
function RuntimeListPointerCallbacks() -> int {
    let xs=[1,2];
    let maps=xs.map((x:int)->map<string,int>{ {"k":x} });
    let objects=xs.map((x:int)->RuntimeListItem{ RuntimeListItem{value:x} });
    let maybe=xs.map((x:int)->int?{if(x==1){null}else{x}});
    let nested=xs.flat_map((x:int)->string[][]{[[x.to_string()]]});
    if(maps[1]["k"]==2 && objects[0].value==1 && maybe[0]==null && (maybe[1] ?? 0)==2 && nested[1][0]=="2"){1}else{-1}
}

class RuntimeListJsonMapBox { values:map<string,(int|float)>[] }
function RuntimeListJsonMaps() -> int {
    let rows=baml.json.decode<RuntimeListJsonMapBox>("{\"values\":[{\"z\":2,\"a\":1.5},{\"tail\":-3}]}");
    let flags=baml.json.decode<map<string,bool>[]>("[{\"ok\":true}]");
    let nested=baml.json.decode<map<string,float[]>[]>("[{\"xs\":[1.5,2.5]}]");
    if(baml.json.stringify(rows)=="{\"values\":[{\"z\":2,\"a\":1.5},{\"tail\":-3}]}" && flags[0]["ok"] && nested[0]["xs"][1]==2.5){1}else{-1}
}
function RuntimeListJsonMapError() -> int {
    baml.json.decode<map<string,int>[]>("[{\"quote\\\"\":[1]}]").length()
}
function RuntimeListJsonMapKeyError() -> int {
    let rows:map<int,string>[]=[{1:"a"}];
    baml.json.stringify(rows).length()
}
class RuntimeListJsonAliasBox { values:json[] }
function RuntimeListJsonAliases() -> int {
    let map:map<string,json> = {"nested":[1,true,"λ"],"nil":null};
    let box=RuntimeListJsonAliasBox{values:[map,2.5,null]};
    if(baml.json.stringify(box,omit_nulls=true)=="{\"values\":[{\"nested\":[1,true,\"λ\"],\"nil\":null},2.5,null]}"){1}else{-1}
}
function RuntimeListJsonMapCycle() -> int {
    let xs:unknown[]=[];
    let map:map<string,unknown> = {"items":xs};
    xs.push(map);
    baml.json.stringify(xs).length()
}

class RuntimeListGenericBox<T> { values:T[] }
function RuntimeListGenericJson() -> int {
    let xs=baml.json.decode<RuntimeListGenericBox<float>[]>("[{\"values\":[1.5,2.5]}]");
    if(baml.json.stringify(xs)=="[{\"values\":[1.5,2.5]}]"){1}else{-1}
}

class RuntimeListNarrowInts { values:int[] }
class RuntimeListNarrowFloats { values:float[] }
function RuntimeListNarrowWalk(v:RuntimeListNarrowInts|RuntimeListNarrowFloats) -> int {
    match(v) {
        RuntimeListNarrowInts=>{
            v.values.push(3);
            v.values[0]=7;
            let total=0;
            for(let x in v.values){total+=x;}
            total
        },
        RuntimeListNarrowFloats=>{
            v.values=[1.5,2.5];
            v.values[1].itrunc()
        },
    }
}
function RuntimeListNarrowedFields() -> int {
    let a=RuntimeListNarrowWalk(RuntimeListNarrowInts{values:[1,2]});
    let b=RuntimeListNarrowWalk(RuntimeListNarrowFloats{values:[9.0]});
    if(a==12 && b==2){1}else{-1}
}

function RuntimeListSortSingletons() -> int {
    let strings:string?[]=[null];
    let ints:int?[]=[null];
    let bools=[true];
    if(baml._rust_sort(strings).length()==1 && baml._rust_sort(ints).length()==1 && baml._rust_sort(bools)[0]){1}else{-1}
}
function RuntimeListSortMutation() -> int {
    let xs=[3,1,2];
    let calls:string[]=[];
    xs.sort_by((a:int,b:int)->baml.ops.Ordering {
        calls.push(a.to_string()+":"+b.to_string());
        xs.push(9);
        a.cmp(b)
    });
    if(xs.join(",")=="1,2,3" && calls.join("|")=="3:1|1:2|3:2"){1}else{-1}
}
function RuntimeListEmptyCallbacks() -> int {
    let xs:int[]=[];
    let calls:int[]=[];
    let f=(x:int)->bool{calls.push(x);true};
    let some=xs.some(f);
    let every=xs.every(f);
    let found=xs.find(f) ?? -1;
    let last=xs.find_last(f) ?? -1;
    let total=xs.reduce((a:int,x:int)->int{calls.push(x);a+x},7);
    let generated=baml.Array.generate(-1,(i:int)->int{calls.push(i);i});
    if(!some && every && found== -1 && last== -1 && total==7 && generated.length()==0 && calls.length()==0){1}else{-1}
}

function RuntimeListNarrowSavedTest(v:RuntimeListNarrowInts|RuntimeListNarrowFloats) -> int {
    let was_ints=v is RuntimeListNarrowInts;
    v=RuntimeListNarrowFloats{values:[2.5]};
    if(was_ints){v.values[0].itrunc()}else{0}
}
function RuntimeListSavedTypeTest() -> int {
    if(RuntimeListNarrowSavedTest(RuntimeListNarrowInts{values:[1]})==2 && RuntimeListNarrowSavedTest(RuntimeListNarrowFloats{values:[1.5]})==0){1}else{-1}
}

// Exercise ropes/views across the growing-list ABI and tagged search/JSON paths.
function RuntimeListRopes() -> int {
    let prefix = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefgh";
    let suffix = "-suffix";
    let rope = prefix + suffix;
    let view = rope.slice(58, 65);
    let values = [rope, prefix, view];
    values.push(prefix + suffix);
    let found = values.includes(prefix + suffix);
    let sorted = baml._rust_sort(values);
    let tagged: (int | string)[] = [1, rope];
    let decoded = baml.json.decode<string[]>(baml.json.stringify(values));
    let mixed: (int | string | float)[] = [1, rope, 2.5];
    let formatted = string.from(mixed);
    let mapping: map<string, int> = {};
    mapping[rope] = 7;
    if (formatted.includes("2.5") && found && tagged.includes(prefix + suffix) && decoded[0] == prefix && sorted[-1] == view && mapping[prefix + suffix] == 7 && values.join("|").length() > 100) {
        string.from_code_points(view.to_code_points()).length()
    } else { -1 }
}
