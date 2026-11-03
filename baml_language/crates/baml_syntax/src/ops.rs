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
    (@at [$($one:ident $a1:tt)*] [$($two:ident $a2:tt $b2:tt)*] [$($three:ident $a3:tt $b3:tt $c3:tt)*]
        $bin:ident = [$a:tt], $($rest:tt)*) => {
        define_operators!(@at [$($one $a1)* $bin $a] [$($two $a2 $b2)*] [$($three $a3 $b3 $c3)*] $($rest)*);
    };
    (@at [$($one:ident $a1:tt)*] [$($two:ident $a2:tt $b2:tt)*] [$($three:ident $a3:tt $b3:tt $c3:tt)*]
        $bin:ident = [$a:tt, $b:tt], $($rest:tt)*) => {
        define_operators!(@at [$($one $a1)*] [$($two $a2 $b2)* $bin $a $b] [$($three $a3 $b3 $c3)*] $($rest)*);
    };
    (@at [$($one:ident $a1:tt)*] [$($two:ident $a2:tt $b2:tt)*] [$($three:ident $a3:tt $b3:tt $c3:tt)*]
        $bin:ident = [$a:tt, $b:tt, $c:tt], $($rest:tt)*) => {
        define_operators!(@at [$($one $a1)*] [$($two $a2 $b2)*] [$($three $a3 $b3 $c3)* $bin $a $b $c] $($rest)*);
    };
    (@at [$($one:ident $a1:tt)*] [$($two:ident $a2:tt $b2:tt)*] [$($three:ident $a3:tt $b3:tt $c3:tt)*]) => {
        impl BinaryOp {
            /// The operator starting at `idx`, longest first: the tokens
            /// right after `idx` (adjacent by construction — every byte is a
            /// token, so a blank between them is a token of its own) complete
            /// a three- or two-token operator, else `idx` alone is one.
            /// `None` if no operator starts here.
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
        }
    };
    (
        binary {
            $( $bp:expr => {
                $( $(#[$bmeta:meta])* $bin:ident = [$($btok:tt),+] ),+ $(,)?
            } ),+ $(,)?
        }
        unary {
            $( $(#[$umeta:meta])* $un:ident = [$utok:tt] ),+ $(,)?
        }
    ) => {
        /// An infix operator, assignment included (the parser treats `a = b`
        /// as a right-associative binary expression; lowering separates it).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum BinaryOp {
            $( $( $(#[$bmeta])* $bin, )+ )+
        }

        impl BinaryOp {
            pub const ALL: &'static [BinaryOp] = &[$( $( Self::$bin, )+ )+];

            /// `(left, right)` binding power: a left-associative operator binds
            /// its right operand one tighter, a right-associative one looser.
            pub const fn binding_power(self) -> (u8, u8) {
                match self { $( $( Self::$bin => $bp, )+ )+ }
            }

            /// The tokens that spell the operator, in order.
            pub const fn tokens(self) -> &'static [TokenKind] {
                match self { $( $( Self::$bin => &[$(TokenKind!($btok)),+], )+ )+ }
            }

            pub const fn spelling(self) -> &'static str {
                match self { $( $( Self::$bin => concat!($($btok),+), )+ )+ }
            }
        }

        define_operators!(@at [] [] [] $( $( $bin = [$($btok),+], )+ )+);

        /// A prefix operator.
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
        (2, 1) => {
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
            ShrAssign = [">", ">", "="],
        },
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
    unary {
        Neg = ["-"],
        Not = ["!"],
        BitNot = ["~"],
        PreInc = ["++"],
        PreDec = ["--"],
    }
}

impl BinaryOp {
    pub const fn is_assignment(self) -> bool {
        self.binding_power().0 == 2
    }

    /// How many tokens spell the operator.
    pub const fn token_len(self) -> usize {
        self.tokens().len()
    }
}

/// A binary operator as it appears in the tree: which one, and the first of
/// its adjacent tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BinaryOpTok {
    pub op: BinaryOp,
    pub first: TokenIdx,
}

impl<A> Walk<A> for BinaryOpTok {
    fn walk(&self, _arena: &A, field: &'static str, visitor: &mut dyn Visitor) {
        for raw in (self.first.0..).take(self.op.token_len()) {
            visitor.token(field, TokenIdx(raw));
        }
    }
}

/// The tokens that can be a prefix operator: `Token![UnaryOpToken]`.
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
