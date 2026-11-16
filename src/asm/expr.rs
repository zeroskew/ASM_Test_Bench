//! Constant expressions (also used for memory operands, where registers may appear).

use super::lexer::{Tok, Token};
use super::{Reg, Size};

#[derive(Clone, Debug)]
pub enum Expr {
    Num(i64),
    Sym(String, (usize, usize)),
    Reg(Reg, (usize, usize)),
    Here,
    SectStart,
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Bin(Tok, Box<Expr>, Box<Expr>, (usize, usize)),
}

pub struct ExprError {
    pub cols: (usize, usize),
    pub message: String,
}

fn err<T>(cols: (usize, usize), message: impl Into<String>) -> Result<T, ExprError> {
    Err(ExprError { cols, message: message.into() })
}

/// Parses an expression from the start of `toks`; returns it and the number of tokens used.
pub fn parse(toks: &[Token]) -> Result<(Expr, usize), ExprError> {
    let mut p = Parser { toks, pos: 0 };
    let e = p.binary(1)?;
    Ok((e, p.pos))
}

struct Parser<'a> {
    toks: &'a [Token],
    pos: usize,
}

fn precedence(t: &Tok) -> Option<u8> {
    Some(match t {
        Tok::Pipe => 1,
        Tok::Caret => 2,
        Tok::Amp => 3,
        Tok::Shl | Tok::Shr => 4,
        Tok::Plus | Tok::Minus => 5,
        Tok::Star | Tok::Slash | Tok::Percent => 6,
        _ => return None,
    })
}

impl Parser<'_> {
    fn end_cols(&self) -> (usize, usize) {
        self.toks.last().map(|t| (t.cols.1, t.cols.1 + 1)).unwrap_or((0, 1))
    }

    fn binary(&mut self, min: u8) -> Result<Expr, ExprError> {
        // Precedence climbing; all binary operators are left-associative.
        let mut lhs = self.unary()?;
        while let Some(t) = self.toks.get(self.pos) {
            let Some(p) = precedence(&t.tok) else { break };
            if p < min {
                break;
            }
            let op = t.tok.clone();
            let cols = t.cols;
            self.pos += 1;
            let rhs = self.binary(p + 1)?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs), cols);
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr, ExprError> {
        let Some(t) = self.toks.get(self.pos) else {
            return err(self.end_cols(), "expected an expression");
        };
        self.pos += 1;
        match &t.tok {
            Tok::Minus => Ok(Expr::Neg(Box::new(self.unary()?))),
            Tok::Plus => self.unary(),
            Tok::Tilde => Ok(Expr::Not(Box::new(self.unary()?))),
            Tok::Num(v) => Ok(Expr::Num(*v)),
            Tok::Str(b) => {
                if b.len() > 4 {
                    return err(t.cols, "character constant longer than 4 bytes");
                }
                let mut v: i64 = 0;
                for (i, &c) in b.iter().enumerate() {
                    v |= (c as i64) << (8 * i);
                }
                Ok(Expr::Num(v))
            }
            Tok::Here => Ok(Expr::Here),
            Tok::SectStart => Ok(Expr::SectStart),
            Tok::Ident(name) => match Reg::parse(name) {
                Some(r) => Ok(Expr::Reg(r, t.cols)),
                None => Ok(Expr::Sym(name.clone(), t.cols)),
            },
            Tok::LParen => {
                let e = self.binary(1)?;
                match self.toks.get(self.pos) {
                    Some(Token { tok: Tok::RParen, .. }) => {
                        self.pos += 1;
                        Ok(e)
                    }
                    Some(t) => err(t.cols, "expected ')'"),
                    None => err(self.end_cols(), "missing ')'"),
                }
            }
            _ => err(t.cols, "expected a number, symbol or '('"),
        }
    }
}

/// Supplies symbol values while evaluating.
pub trait Resolve {
    fn symbol(&self, name: &str) -> Result<i64, String>;
    fn here(&self) -> i64;
    fn sect_start(&self) -> i64;
}

/// `konst + sum(coeff * reg)`, the general shape of an address expression.
#[derive(Clone, Debug, Default)]
pub struct Linear {
    pub konst: i64,
    pub regs: Vec<(u8, i64, (usize, usize))>,
}

impl Linear {
    fn constant(v: i64) -> Self {
        Linear { konst: v, regs: Vec::new() }
    }
    fn scale(mut self, k: i64) -> Self {
        self.konst = self.konst.wrapping_mul(k);
        for r in &mut self.regs {
            r.1 *= k;
        }
        self
    }
}

pub fn eval(e: &Expr, ctx: &dyn Resolve) -> Result<Linear, ExprError> {
    Ok(match e {
        Expr::Num(v) => Linear::constant(*v),
        Expr::Here => Linear::constant(ctx.here()),
        Expr::SectStart => Linear::constant(ctx.sect_start()),
        Expr::Sym(name, cols) => match ctx.symbol(name) {
            Ok(v) => Linear::constant(v),
            Err(m) => return err(*cols, m),
        },
        Expr::Reg(r, cols) => {
            if r.size != Size::D {
                return err(*cols, format!("only 32-bit registers can be used in an address, not '{}'", r.name()));
            }
            Linear { konst: 0, regs: vec![(r.idx, 1, *cols)] }
        }
        Expr::Neg(x) => eval(x, ctx)?.scale(-1),
        Expr::Not(x) => {
            let v = eval(x, ctx)?;
            if let Some((_, _, c)) = v.regs.first() {
                return err(*c, "a register can't be used here");
            }
            Linear::constant(!v.konst)
        }
        Expr::Bin(op, a, b, cols) => {
            let a = eval(a, ctx)?;
            let b = eval(b, ctx)?;
            match op {
                Tok::Plus => {
                    let mut r = a;
                    r.konst = r.konst.wrapping_add(b.konst);
                    r.regs.extend(b.regs);
                    r
                }
                Tok::Minus => {
                    let b = b.scale(-1);
                    let mut r = a;
                    r.konst = r.konst.wrapping_add(b.konst);
                    r.regs.extend(b.regs);
                    r
                }
                Tok::Star => {
                    if a.regs.is_empty() {
                        b.scale(a.konst)
                    } else if b.regs.is_empty() {
                        a.scale(b.konst)
                    } else {
                        return err(*cols, "can't multiply two registers");
                    }
                }
                _ => {
                    if let Some((_, _, c)) = a.regs.first().or(b.regs.first()) {
                        return err(*c, "a register can't be used with this operator");
                    }
                    let (x, y) = (a.konst, b.konst);
                    Linear::constant(match op {
                        Tok::Slash | Tok::Percent if y == 0 => return err(*cols, "division by zero in expression"),
                        Tok::Slash => x / y,
                        Tok::Percent => x % y,
                        Tok::Shl => x.wrapping_shl(y as u32),
                        Tok::Shr => ((x as u64).wrapping_shr(y as u32)) as i64,
                        Tok::Amp => x & y,
                        Tok::Pipe => x | y,
                        Tok::Caret => x ^ y,
                        _ => unreachable!(),
                    })
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::super::lexer::tokenize;
    use super::*;

    struct Ctx;
    impl Resolve for Ctx {
        fn symbol(&self, name: &str) -> Result<i64, String> {
            if name == "x" {
                Ok(100)
            } else {
                Err(format!("undefined symbol '{name}'"))
            }
        }
        fn here(&self) -> i64 {
            110
        }
        fn sect_start(&self) -> i64 {
            0
        }
    }

    fn val(s: &str) -> i64 {
        let t = tokenize(s).ok().unwrap();
        let (e, n) = parse(&t).ok().unwrap();
        assert_eq!(n, t.len());
        eval(&e, &Ctx).ok().unwrap().konst
    }

    #[test]
    fn arithmetic() {
        assert_eq!(val("1 + 2 * 3"), 7);
        assert_eq!(val("(1 + 2) * 3"), 9);
        assert_eq!(val("10 - 2 - 3"), 5);
        assert_eq!(val("$ - x"), 10);
        assert_eq!(val("1 << 4 | 1"), 17);
        assert_eq!(val("-4 + 'A'"), 61);
        assert_eq!(val("100 / 10 / 5"), 2);
    }

    #[test]
    fn address() {
        let t = tokenize("ebx + ecx*4 + x + 8").ok().unwrap();
        let (e, _) = parse(&t).ok().unwrap();
        let l = eval(&e, &Ctx).ok().unwrap();
        assert_eq!(l.konst, 108);
        assert_eq!(l.regs.iter().map(|r| (r.0, r.1)).collect::<Vec<_>>(), vec![(3, 1), (1, 4)]);
    }
}
