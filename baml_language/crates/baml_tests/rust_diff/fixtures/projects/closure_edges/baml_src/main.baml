interface Supply<T> { function get(self)->T throws never; }
class SupplyBox<T> {value:T, implements Supply<T> {function get(self)->T throws never {self.value}}}
function bind_supply<T>(s:Supply<T>)->()->T throws never {s.get}
function RtVirtualGenericClosures()->int {
    let box=SupplyBox<int> {value:37};
    let get=bind_supply<int>(box);
    let unbound=(SupplyBox<int> as Supply<int>).get;
    get()+unbound(box)
}
class GenericPicker<T> {
    value:T,
    function pick<U>(self,n:U)->U {n}
}
function RtBoundGenericClosures()->int {
    let box=GenericPicker<string> {value:"kept"};
    let f=box.pick<int>;
    let g:(int)->int throws never=box.pick;
    if (f!=g || f==box.pick<string>) {return -1;}
    f(17)+g(20)
}
interface Picker {function pick<U>(self,n:U)->U throws never;}
class PickerBox {implements Picker {function pick<U>(self,n:U)->U throws never {n}}}
function bind_picker(p:Picker)->(int)->int throws never {p.pick}
function RtVirtualOwnGenericClosure()->int {let p=PickerBox {}; let f=bind_picker(p); f(42)}

function RtNativeHigherOrderValue()->int {
    let values=[1,2,3];
    let map:((int)->int throws never)->int[] throws never=values.map;
    let output=map((n:int)->int {n*2});
    let reduce:((int,int)->int throws never,int)->int throws never=output.reduce;
    let flags=[true,true,false,true];
    let map_flags:((bool)->bool throws never)->bool[] throws never=flags.map;
    let inverted=map_flags((value:bool)->bool {!value});
    if(inverted[0] || inverted[1] || !inverted[2] || inverted[3]) { return -1; }
    reduce((a:int,b:int)->int {a+b},0)
}

function callback_default<T>(x:T,tag:string="tag")->T {x}
function add_default(x:int,delta:int=5)->int {x+delta}
function RtDefaultCallbacks()->int {
    let generic:(int)->int throws never=callback_default<int>;
    let f:(int)->int throws never=add_default;
    let values=[1,2].map(callback_default<int>);
    f(generic(20))+values[0]+values[1]
}
