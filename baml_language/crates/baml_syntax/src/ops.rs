//! Operators and their precedence: the one table the parser, the formatter
//! (for redundant-parenthesis decisions) and lowering all read.
//!
//! Each operator is declared once, with the tokens that spell it and its
//! binding power; everything else is derived from that declaration. An
//! operator spelled by more than one token (`?` `?`, `>` `=`, `>` `>` `=`) is
//! read from *adjacent* tokens: `?` and `>` are always one character each,
//! because `int??` needs two `?` and a nested generic's `>>`, `>=` or `>>=`
//! must close two lists.

use crate::{
    token::{KindSet, TokenIdx, TokenKind, TokenSet, Tokens},
    tree::{Visitor, Walk},
};

/// `is`: same precedence as the comparisons.
pub const IS_BINDING_POWER: (u8, u8) = (18, 19);

/// The operand binding power of a prefix operator.
//
// BUG: this is below `*`/`/`/`%` (24, 25), so `-a * b` parses as `-(a * b)`.
// Kept as-is so the typed parser matches the current one exactly; fix it in
// its own change (with the lowering and formatter tests that pin the result).
pub const PREFIX_BINDING_POWER: u8 = 23;

macro_rules! define_operators {
    // Split the flattened operator list by how many tokens spell each
    // operator. There is no rule for four or more: such a spelling is a
    // compile error, which keeps the generated matcher complete.
    (@at $ty:ident [$($one:ident $a1:tt)*] [$($two:ident $a2:tt $b2:tt)*] [$($three:ident $a3:tt $b3:tt $c3:tt)*]
        $op:ident = [$a:tt], $($rest:tt)*) => {
        define_operators!(@at $ty [$($one $a1)* $op $a] [$($two $a2 $b2)*] [$($three $a3 $b3 $c3)*] $($rest)*);
    };
    (@at $ty:ident [$($one:ident $a1:tt)*] [$($two:ident $a2:tt $b2:tt)*] [$($three:ident $a3:tt $b3:tt $c3:tt)*]
        $op:ident = [$a:tt, $b:tt], $($rest:tt)*) => {
        define_operators!(@at $ty [$($one $a1)*] [$($two $a2 $b2)* $op $a $b] [$($three $a3 $b3 $c3)*] $($rest)*);
    };
    (@at $ty:ident [$($one:ident $a1:tt)*] [$($two:ident $a2:tt $b2:tt)*] [$($three:ident $a3:tt $b3:tt $c3:tt)*]
        $op:ident = [$a:tt, $b:tt, $c:tt], $($rest:tt)*) => {
        define_operators!(@at $ty [$($one $a1)*] [$($two $a2 $b2)*] [$($three $a3 $b3 $c3)* $op $a $b $c] $($rest)*);
    };
    (@at $ty:ident [$($one:ident $a1:tt)*] [$($two:ident $a2:tt $b2:tt)*] [$($three:ident $a3:tt $b3:tt $c3:tt)*]) => {
        impl $ty {
            /// The operator spelled by the tokens starting at `idx`, if any:
            /// the longest spelling wins.
            pub fn at(tokens: &Tokens, idx: TokenIdx) -> Option<Self> {
                let kind_at = |offset: u32| tokens.get(TokenIdx(idx.0 + offset));
                match (tokens.kind(idx), kind_at(1), kind_at(2)) {
                    $( (TokenKind!($a3), Some(TokenKind!($b3)), Some(TokenKind!($c3))) => return Some(Self::$three), )*
                    _ => {}
                }
                match (tokens.kind(idx), kind_at(1)) {
                    $( (TokenKind!($a2), Some(TokenKind!($b2))) => return Some(Self::$two), )*
                    _ => {}
                }
                match tokens.kind(idx) {
                    $( TokenKind!($a1) => Some(Self::$one), )*
                    _ => None,
                }
            }

            pub const fn token_len(self) -> usize {
                self.tokens().len()
            }
        }

        impl Operator for $ty {
            fn at(tokens: &Tokens, idx: TokenIdx) -> Option<Self> {
                Self::at(tokens, idx)
            }

            fn token_len(self) -> usize {
                self.token_len()
            }
        }
    };
    // The members shared by every operator kind spelled by one or more
    // tokens.
    (@spelled $ty:ident { $( $op:ident = [$($tok:tt),+] ),+ $(,)? }) => {
        impl $ty {
            pub const ALL: &'static [$ty] = &[$( Self::$op, )+];

            pub const fn tokens(self) -> &'static [TokenKind] {
                match self { $( Self::$op => &[$(TokenKind!($tok)),+], )+ }
            }

            pub const fn spelling(self) -> &'static str {
                match self { $( Self::$op => concat!($($tok),+), )+ }
            }
        }

        define_operators!(@at $ty [] [] [] $( $op = [$($tok),+], )+);
    };
    (
        binary {
            $( $bp:expr => {
                $( $(#[$bmeta:meta])* $bin:ident = [$($btok:tt),+] ),+ $(,)?
            } ),+ $(,)?
        }
        assign {
            $( $(#[$ameta:meta])* $asg:ident = [$($atok:tt),+] ),+ $(,)?
        }
        unary {
            $( $(#[$umeta:meta])* $un:ident = [$utok:tt] ),+ $(,)?
        }
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum BinaryOp {
            $( $( $(#[$bmeta])* $bin, )+ )+
        }

        impl BinaryOp {
            pub const fn binding_power(self) -> (u8, u8) {
                match self { $( $( Self::$bin => $bp, )+ )+ }
            }
        }

        define_operators!(@spelled BinaryOp { $( $( $bin = [$($btok),+], )+ )+ });

        /// `=` and the compound assignments. Not binary operators: an
        /// assignment is a statement ([`AssignStmt`](crate::nodes::AssignStmt)),
        /// never a value, so the expression grammar does not read these.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum AssignOp {
            $( $(#[$ameta])* $asg, )+
        }

        define_operators!(@spelled AssignOp { $( $asg = [$($atok),+], )+ });

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum UnaryOp {
            $( $(#[$umeta])* $un, )+
        }

        impl UnaryOp {
            pub const ALL: &'static [UnaryOp] = &[$( Self::$un, )+];

            pub const fn token(self) -> TokenKind {
                match self { $( Self::$un => TokenKind!($utok), )+ }
            }

            pub const fn spelling(self) -> &'static str {
                match self { $( Self::$un => $utok, )+ }
            }

            pub const fn from_kind(kind: TokenKind) -> Option<Self> {
                match kind {
                    $( TokenKind!($utok) => Some(Self::$un), )+
                    _ => None,
                }
            }
        }
    };
}

define_operators! {
    binary {
        (4, 5) => {
            /// `a ?? b`: `a` unless it is null, else `b`.
            NullCoalesce = ["?", "?"],
        },
        (6, 7) => { Or = ["||"] },
        (8, 9) => { And = ["&&"] },
        (10, 11) => { BitOr = ["|"] },
        (12, 13) => { BitXor = ["^"] },
        (14, 15) => { BitAnd = ["&"] },
        (16, 17) => { Eq = ["=="], Ne = ["!="] },
        IS_BINDING_POWER => {
            Lt = ["<"],
            Gt = [">"],
            Le = ["<="],
            Ge = [">", "="],
            Instanceof = ["instanceof"],
        },
        (20, 21) => { Shl = ["<<"], Shr = [">", ">"] },
        (22, 23) => { Add = ["+"], Sub = ["-"] },
        (24, 25) => { Mul = ["*"], Div = ["/"], Mod = ["%"] },
    }
    assign {
        Assign = ["="],
        AddAssign = ["+="],
        SubAssign = ["-="],
        MulAssign = ["*="],
        DivAssign = ["/="],
        ModAssign = ["%="],
        BitAndAssign = ["&="],
        BitOrAssign = ["|="],
        BitXorAssign = ["^="],
        ShlAssign = ["<<="],
        /// Three tokens: `>` is lexed one character at a time (D3), and the
        /// parser asks for an assignment before a binary operator so this
        /// is never read as `>>` followed by `=`.
        ShrAssign = [">", ">", "="],
    }
    unary {
        Neg = ["-"],
        Not = ["!"],
        BitNot = ["~"],
        PreInc = ["++"],
        PreDec = ["--"],
    }
}

/// An operator kind spelled by one or more adjacent tokens.
pub trait Operator: Copy {
    /// The operator spelled by the tokens starting at `idx`, if any.
    fn at(tokens: &Tokens, idx: TokenIdx) -> Option<Self>;
    /// How many tokens spell it.
    fn token_len(self) -> usize;
}

/// An operator of any family: what a tool that paints or counts operators
/// without parsing asks for. Assignment is tried first so `>>=` is one
/// operator rather than `>>` and a stray `=`; a one-token spelling shared by
/// two families (`-`) reads as the binary one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyOperator {
    Binary(BinaryOp),
    Assign(AssignOp),
    Unary(UnaryOp),
}

impl Operator for AnyOperator {
    fn at(tokens: &Tokens, idx: TokenIdx) -> Option<Self> {
        AssignOp::at(tokens, idx)
            .map(Self::Assign)
            .or_else(|| BinaryOp::at(tokens, idx).map(Self::Binary))
            .or_else(|| {
                let kind = tokens.get(idx)?;
                UnaryOp::ALL
                    .iter()
                    .copied()
                    .find(|op| op.token() == kind)
                    .map(Self::Unary)
            })
    }

    fn token_len(self) -> usize {
        match self {
            Self::Binary(op) => op.token_len(),
            Self::Assign(op) => op.token_len(),
            Self::Unary(_) => 1,
        }
    }
}

/// An operator as it sits in the token array: which one, and its first
/// token (the others are the adjacent tokens after it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpTok<O> {
    op: O,
    first: TokenIdx,
}

pub type BinaryOpTok = OpTok<BinaryOp>;
pub type AssignOpTok = OpTok<AssignOp>;

impl<O: Operator> OpTok<O> {
    pub fn at(tokens: &Tokens, idx: TokenIdx) -> Option<Self> {
        O::at(tokens, idx).map(|op| Self { op, first: idx })
    }

    pub fn op(self) -> O {
        self.op
    }

    pub fn first(self) -> TokenIdx {
        self.first
    }

    pub fn tokens(self) -> impl Iterator<Item = TokenIdx> {
        (self.first.0..).take(self.op.token_len()).map(TokenIdx)
    }
}

impl<A, O: Operator> Walk<A> for OpTok<O> {
    fn walk(&self, _arena: &A, field: &'static str, visitor: &mut dyn Visitor) {
        for token in self.tokens() {
            visitor.token(field, token);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnaryOpToken;

impl TokenSet for UnaryOpToken {
    const KINDS: KindSet = {
        let mut set = KindSet::EMPTY;
        let mut i = 0;
        while i < UnaryOp::ALL.len() {
            set = set.insert(UnaryOp::ALL[i].token());
            i += 1;
        }
        set
    };
}
