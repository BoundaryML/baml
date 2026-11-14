
type Chain = (int, Chain?)
type ChainOrNull = (int, ChainOrNull) | null

type Tree = (int, Tree[])

type Endless = (int, Endless)

type Bad = (Bad, int?)

type HeadFirst = (HeadFirst, HeadFirst[])

type Ping = (int, Pong)
type Pong = (Ping, string)

type There = (int, Back)
type Back = There | null
