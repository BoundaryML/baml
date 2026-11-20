type A = string /* c */ []?;
type B = string[ /* c */ ]?;
type C = string /* c */ []?[];
type D = map<string, int /* c */ []?>;
type E = int /* 0 */ ? // 1
/* 2 */ // 3
;
type F = int // 1
[] // 2
;
