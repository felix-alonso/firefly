///! Purpose : Transform normal Erlang to Core Erlang
///!
///! At this stage all preprocessing has been done. All that is left are
///! "pure" Erlang functions.
///!
///! Core transformation is done in four stages:
///!
///! 1. Flatten expressions into an internal core form without doing
///!    matching.
///!
///! 2. Step "forwards" over the icore code annotating each "top-level"
///!    thing with variable usage.  Detect bound variables in matching
///!    and replace with explicit guard test.  Annotate "internal-core"
///!    expressions with variables they use and create.  Convert matches
///!    to cases when not pure assignments.
///!
///! 3. Step "backwards" over icore code using variable usage
///!    annotations to change implicit exported variables to explicit
///!    returns.
///!
///! 4. Lower receives to more primitive operations.  Split binary
///!    patterns where a value is matched out and then used used as
///!    a size in the same pattern.  That simplifies the subsequent
///!    passes as all variables are within a single pattern are either
///!    new or used, but never both at the same time.
///!
///! To ensure the evaluation order we ensure that all arguments are
///! safe.  A "safe" is basically a core_lib simple with VERY restricted
///! binaries.
///!
///! We have to be very careful with matches as these create variables.
///! While we try not to flatten things more than necessary we must make
///! sure that all matches are at the top level.  For this we use the
///! type "novars" which are non-match expressions.  Cases and receives
///! can also create problems due to exports variables so they are not
///! "novars" either.  I.e. a novars will not export variables.
///!
///! Annotations in the #iset, #iletrec, and all other internal records
///! is kept in a record, #a, not in a list as in proper core.  This is
///! easier and faster and creates no problems as we have complete control
///! over all annotations.
///!
///! On output, the annotation for most Core Erlang terms will contain
///! the source line number. A few terms will be marked with the atom
///! atom 'compiler_generated', to indicate that the compiler has generated
///! them and that no warning should be generated if they are optimized
///! away.
///!
///!
///! In this translation:
///!
///! call ops are safes
///! call arguments are safes
///! match arguments are novars
///! case arguments are novars
///! receive timeouts are novars
///! binaries and maps are novars
///! let/set arguments are expressions
///! fun is not a safe
use std::cell::UnsafeCell;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use rpds::{RedBlackTreeSet, Vector};

use liblumen_binary::{BinaryEntrySpecifier, BitVec};
use liblumen_diagnostics::*;
use liblumen_intern::{symbols, Ident, Symbol};
use liblumen_number::Integer;
use liblumen_pass::Pass;

use crate::ast;
use crate::ast::{BinaryOp, UnaryOp};
use crate::cst::{self, *};
use crate::evaluator;
use crate::Arity;

macro_rules! lit_atom {
    ($span:expr, $sym:expr) => {
        Literal::atom($span, $sym)
    };
}

macro_rules! lit_int {
    ($span:expr, $i:expr) => {
        Literal::integer($span, $i)
    };
}

macro_rules! lit_tuple {
    ($span:expr, $($element:expr),*) => {
        Literal::tuple($span, vec![$($element),*])
    };
}

macro_rules! lit_nil {
    ($span:expr) => {
        Literal::nil($span)
    };
}

macro_rules! iatom {
    ($span:expr, $sym:expr) => {
        IExpr::Literal(lit_atom!($span, $sym))
    };
}

macro_rules! iint {
    ($span:expr, $i:expr) => {
        IExpr::Literal(lit_int!($span, $i))
    };
}

macro_rules! ituple {
    ($span:expr, $($element:expr),*) => {
        IExpr::Tuple(ITuple::new($span, vec![$($element),*]))
    };
}

macro_rules! icons {
    ($span:expr, $head:expr, $tail:expr) => {
        IExpr::Cons(ICons::new($span, $head, $tail))
    };
}

macro_rules! inil {
    ($span:expr) => {
        IExpr::Literal(lit_nil!($span))
    };
}

macro_rules! icall_eq_true {
    ($span:expr, $v:expr) => {{
        let span = $span;
        IExpr::Call(ICall {
            span,
            annotations: Annotations::default_compiler_generated(),
            module: Box::new(iatom!(span, symbols::Erlang)),
            function: Box::new(iatom!(span, symbols::EqualStrict)),
            args: vec![$v, iatom!(span, symbols::True)],
        })
    }};
}

#[derive(Debug, PartialEq)]
struct FunctionContext {
    span: SourceSpan,
    var_counter: usize,
    fun_counter: usize,
    goto_counter: usize,
    name: Ident,
    arity: u8,
    wanted: bool,
    in_guard: bool,
    is_nif: bool,
}

mod annotate;
mod rewrites;
mod simplify;
mod translate;

pub use self::translate::AstToCst;

impl FunctionContext {
    fn new(f: &ast::Function) -> Self {
        Self {
            span: f.span,
            var_counter: f.var_counter,
            fun_counter: f.fun_counter,
            goto_counter: 0,
            name: f.name,
            arity: f.arity,
            wanted: true,
            in_guard: false,
            is_nif: false,
        }
    }

    #[inline]
    fn set_wanted(&mut self, wanted: bool) -> bool {
        let prev = self.wanted;
        self.wanted = wanted;
        prev
    }

    fn next_var_name(&mut self, span: Option<SourceSpan>) -> Ident {
        let id = self.var_counter;
        self.var_counter += 1;
        let var = format!("${}", id);
        let mut ident = Ident::from_str(&var);
        if let Some(span) = span {
            ident.span = span;
        }
        ident
    }

    fn next_var(&mut self, span: Option<SourceSpan>) -> Var {
        let name = self.next_var_name(span);
        Var {
            annotations: Annotations::default_compiler_generated(),
            name,
            arity: None,
        }
    }

    fn next_n_vars(&mut self, n: usize, span: Option<SourceSpan>) -> Vec<Var> {
        (0..n).map(|_| self.next_var(span)).collect()
    }

    fn new_fun_name(&mut self, ty: Option<&str>) -> Symbol {
        let name = if let Some(ty) = ty {
            format!("{}$^{}", ty, self.fun_counter)
        } else {
            format!(
                "-{}/{}-fun-{}-",
                self.name.name, self.arity, self.fun_counter
            )
        };
        self.fun_counter += 1;
        Symbol::intern(&name)
    }

    fn goto_func(&self) -> Var {
        let sym = Symbol::intern(&format!("label^{}", self.goto_counter));
        Var::new_with_arity(Ident::with_empty_span(sym), Arity::Int(0))
    }

    fn inc_goto_func(&mut self) {
        self.goto_counter += 1;
    }
}

/// Here follows an abstract data structure to help us handle Erlang's
/// implicit matching that occurs when a variable is bound more than
/// once:
///
///     X = Expr1(),
///     X = Expr2()
///
/// What is implicit in Erlang, must be explicit in Core Erlang; that
/// is, repeated variables must be eliminated and explicit matching
/// must be added. For simplicity, examples that follow will be given
/// in Erlang and not in Core Erlang. Here is how the example can be
/// rewritten in Erlang to eliminate the repeated variable:
///
///     X = Expr1(),
///     X1 = Expr2(),
///     if
///         X1 =:= X -> X;
///         true -> error({badmatch,X1})
///     end
///
/// To implement the renaming, keeping a set of the variables that
/// have been bound so far is **almost** sufficient. When a variable
/// in the set is bound a again, it will be renamed and a `case` with
/// guard test will be added.
///
/// Here is another example:
///
///     (X=A) + (X=B)
///
/// Note that the operands for a binary operands are allowed to be
/// evaluated in any order. Therefore, variables bound on the left
/// hand side must not referenced on the right hand side, and vice
/// versa. If a variable is bound on both sides, it must be bound
/// to the same value.
///
/// Using the simple scheme of keeping track of known variables,
/// the example can be rewritten like this:
///
///     X = A,
///     X1 = B,
///     if
///         X1 =:= X -> ok;
///         true -> error({badmatch,X1})
///     end,
///     X + X1
///
/// However, this simple scheme of keeping all previously bound variables in
/// a set breaks down for this example:
///
///     (X=A) + fun() -> X = B end()
///
/// The rewritten code would be:
///
///     X = A,
///     Tmp = fun() ->
///               X1 = B,
///               if
///                   X1 =:= X -> ok;
///                   true -> error({badmatch,X1})
///               end
///           end(),
///     X + Tmp
///
/// That is wrong, because the binding of `X` created on the left hand
/// side of `+` must not be seen inside the fun. The correct rewrite
/// would be like this:
///
///     X = A,
///     Tmp = fun() ->
///               X1 = B
///           end(),
///     X + Tmp
///
/// To correctly rewrite fun bodies, we will need to keep addtional
/// information in a record so that we can remove `X` from the known
/// variables when rewriting the body of the fun.
///
#[derive(Clone, Default)]
struct Known {
    base: Vector<RedBlackTreeSet<Ident>>,
    ks: RedBlackTreeSet<Ident>,
    prev_ks: Vector<RedBlackTreeSet<Ident>>,
}
impl Known {
    /// Get the currently known variables
    fn get(&self) -> &RedBlackTreeSet<Ident> {
        &self.ks
    }

    /// Returns true if the given ident is known in the current scope
    #[inline]
    fn contains(&self, id: &Ident) -> bool {
        self.ks.contains(id)
    }

    /// Returns true if the known set is empty
    fn is_empty(&self) -> bool {
        self.ks.is_empty()
    }

    fn start_group(&mut self) {
        self.prev_ks.push_back_mut(RedBlackTreeSet::new());
        self.base.push_back_mut(self.ks.clone());
    }

    fn end_body(&mut self) {
        self.prev_ks.drop_last_mut();
        self.prev_ks.push_back_mut(self.ks.clone());
    }

    /// known_end_group(#known{}) -> #known{}.
    ///  Consolidate the known variables after having processed the
    ///  last body in a group of bodies that see the same bindings.
    fn end_group(&mut self) {
        self.base.drop_last_mut();
        self.prev_ks.drop_last_mut();
    }

    /// known_union(#known{}, KnownVarsSet) -> #known{}.
    ///  Update the known variables to be the union of the previous
    ///  known variables and the set KnownVarsSet.
    fn union(&self, vars: &RedBlackTreeSet<Ident>) -> Self {
        let ks = vars
            .iter()
            .copied()
            .fold(self.ks.clone(), |ks, var| ks.insert(var));
        Self {
            base: self.base.clone(),
            ks,
            prev_ks: self.prev_ks.clone(),
        }
    }

    /// known_bind(#known{}, BoundVarsSet) -> #known{}.
    ///  Add variables that are known to be bound in the current
    ///  body.
    fn bind(&self, vars: &RedBlackTreeSet<Ident>) -> Self {
        let last = self.prev_ks.last().map(|set| set.clone());
        let prev_ks = self.prev_ks.drop_last();
        match last {
            None => self.clone(),
            Some(mut last) => {
                let mut prev_ks = prev_ks.unwrap();
                // set difference of prev_ks and vars
                for v in vars.iter() {
                    last.remove_mut(v);
                }
                prev_ks.push_back_mut(last);
                Self {
                    base: self.base.clone(),
                    ks: self.ks.clone(),
                    prev_ks,
                }
            }
        }
    }

    /// known_in_fun(#known{}) -> #known{}.
    ///  Update the known variables to only the set of variables that
    ///  should be known when entering the fun.
    fn known_in_fun(&self) -> Self {
        if self.base.is_empty() || self.prev_ks.is_empty() {
            return self.clone();
        }

        // Within a group of bodies that see the same bindings, calculate
        // the known variables for a fun. Example:
        //
        //     A = 1,
        //     {X = 2, fun() -> X = 99, A = 1 end()}.
        //
        // In this example:
        //
        //     BaseKs = ['A'], Ks0 = ['A','X'], PrevKs = ['A','X']
        //
        // Thus, only `A` is known when entering the fun.
        let mut ks = self.ks.clone();
        let prev_ks = self.prev_ks.last().map(|l| l.clone()).unwrap_or_default();
        let base = self.base.last().map(|l| l.clone()).unwrap_or_default();
        for id in prev_ks.iter() {
            ks.remove_mut(id);
        }
        for id in base.iter() {
            ks.insert_mut(*id);
        }
        Self {
            base: Vector::new(),
            prev_ks: Vector::new(),
            ks,
        }
    }
}

pub(self) fn used_in_any<'a, A: Annotated + 'a, I: Iterator<Item = &'a A>>(
    iter: I,
) -> RedBlackTreeSet<Ident> {
    iter.fold(RedBlackTreeSet::new(), |used, annotated| {
        union(annotated.used_vars(), used)
    })
}

pub(self) fn new_in_any<'a, A: Annotated + 'a, I: Iterator<Item = &'a A>>(
    iter: I,
) -> RedBlackTreeSet<Ident> {
    iter.fold(RedBlackTreeSet::new(), |new, annotated| {
        union(annotated.new_vars(), new)
    })
}

pub(self) fn new_in_all<'a, A: Annotated + 'a, I: Iterator<Item = &'a A>>(
    iter: I,
) -> RedBlackTreeSet<Ident> {
    iter.fold(None, |new, annotated| match new {
        None => Some(annotated.new_vars().clone()),
        Some(ns) => Some(intersection(annotated.new_vars(), ns)),
    })
    .unwrap_or_default()
}

pub(self) fn union(x: RedBlackTreeSet<Ident>, y: RedBlackTreeSet<Ident>) -> RedBlackTreeSet<Ident> {
    let mut result = x;
    for id in y.iter().copied() {
        result.insert_mut(id);
    }
    result
}

pub(self) fn subtract(
    x: RedBlackTreeSet<Ident>,
    y: RedBlackTreeSet<Ident>,
) -> RedBlackTreeSet<Ident> {
    let mut result = x;
    for id in y.iter() {
        result.remove_mut(id);
    }
    result
}

pub(self) fn intersection(
    x: RedBlackTreeSet<Ident>,
    y: RedBlackTreeSet<Ident>,
) -> RedBlackTreeSet<Ident> {
    let mut result = RedBlackTreeSet::new();
    for id in x.iter().copied() {
        if y.contains(&id) {
            result.insert_mut(id);
        }
    }
    result
}