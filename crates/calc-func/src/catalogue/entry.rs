//! Formula entry: what Excel's formula entry accepts in each parameter of an entry, what a call
//! of the entry may give, and the check a formula's compiler asks once per call
//! ([`FunctionCallTarget::check_entry`]).
//!
//! Excel checks a formula when it is entered and refuses some outright, before storing anything:
//! COM's `Range.Formula` and `Range.Formula2` then throw `0x800A03EC`, the face of the "There's
//! a problem with this formula" dialog. The check reads the formula's syntax only, never a value:
//! an unknown name enters (and gives `#NAME?`), and so does a value of the wrong type (which
//! gives `#VALUE!` when it is calculated). The rules here were measured on Excel 16.0 build
//! 20527 on 2026-11-07 (dc-mx3x), each formula assigned through `Formula2` and through `Formula`
//! to a cell of a new workbook whose A1 and B1:B3 held numbers; the two forms agreed on every
//! one of 3,822 formulas. COM says only "taken" or "refused", never which rule refused, so a rule
//! name is this catalogue's explanation, held to the refused cases and their accepted
//! neighbours. CalcFml's test `entry_matches_excel` replays every answer.
//!
//! - **Argument counts.** A call outside its entry's arity is refused: `=IRR()`, `=SUM()`,
//!   `=PI(1)`, `=COUNTBLANK(A1,A2)`. Repeating groups must be complete
//!   ([`ArgumentGroups`]).
//! - **Reference places** ([`EntryClass::Reference`]). An operand there must be able to give a
//!   reference: a reference or a reference operation (range, intersection, union, spill), a
//!   name (`COUNTBLANK(nm)` enters even when `nm` is `=1`, and so does an undefined name), a
//!   LAMBDA, LET or a call of a value, or a call of an entry whose [`ResultForm`] is not
//!   [`ResultForm::Value`]. A constant, an array constant, an omitted argument, an operation of
//!   values (`-A1`, `A1+0`, `A1&""`, `A1%`) or a call of an entry that gives a value
//!   (`SUM(A1:A2)`, `TRUE()`) is refused, also in parentheses: `=COUNTBLANK((2))` and
//!   `=COUNTBLANK((1+1))` are refused, `=COUNTBLANK((A1:A2))` enters.
//! - **Called results.** A call postfix `(...)` may follow an operand that may be called: the
//!   same forms as a reference place accepts (a reference enters and gives `#REF!`: `=A1(3)`,
//!   `=INDEX(A1:A2,1)(3)`), so a call of an entry that gives a value is refused
//!   ([`EntryRule::ResultNotCallable`]): `=IFERROR(#N/A,LAMBDA(x,x))(3)`, `=SUM(1)(3)`,
//!   `=MAP({1},LAMBDA(x,x))(3)`, while `=IF(TRUE,LAMBDA(x,x))(3)` is 3. Which other heads the
//!   grammar refuses (`=(1)(3)`, `={1}(3)`) is CalcFml's.
//!
//! Excel's entry takes any operand in a LAMBDA place: `=MAP({1},1)`, `=REDUCE(0,{1},1)` and
//! `=GROUPBY(B1:B3,B1:B3,1)` enter and give `#VALUE!`, so no entry class requires a callable.

use super::{FunctionCallTarget, FunctionId, catalogue};
use std::fmt;

/// What Excel's formula entry accepts in a parameter. The default is [`EntryClass::Any`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntryClass {
    /// Every operand form.
    Any,
    /// Only an operand that may give a reference ([`OperandForm::fills_reference_place`]).
    Reference,
}

/// What a call of an entry may give, as Excel's formula entry reads it. The default is
/// [`ResultForm::Value`]. Entry treats both other forms alike: such a call fills a reference
/// place and may be called.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResultForm {
    /// A value: never a reference, never a LAMBDA.
    Value,
    /// A value or a reference (INDEX, OFFSET, INDIRECT, XLOOKUP).
    MayBeReference,
    /// A value, a reference or a LAMBDA: one of its arguments, as IF and CHOOSE return.
    MayBeEither,
}

impl ResultForm {
    /// Whether the result may be a reference or a LAMBDA.
    pub const fn may_be_reference_or_callable(self) -> bool {
        !matches!(self, ResultForm::Value)
    }
}

/// An entry's repeating groups of arguments: from argument `from` on, the arguments come in groups
/// of `size`, each argument taking the declared parameter at `from + (i - from) % size`, and at
/// most `tail` arguments may follow the last complete group (SWITCH's default). The first group's
/// completeness is the arity's: SORTBY takes `(array, by_array1)` without its sort order, but
/// not a later group without one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArgumentGroups {
    pub(crate) from: usize,
    pub(crate) size: usize,
    pub(crate) tail: usize,
}

impl ArgumentGroups {
    pub(crate) const fn new(from: usize, size: usize, tail: usize) -> Self {
        ArgumentGroups { from, size, tail }
    }

    /// Whether `count` arguments end in an incomplete group.
    pub(crate) const fn incomplete(self, count: usize) -> bool {
        let rest = count.saturating_sub(self.from);
        rest > self.size && rest % self.size > self.tail
    }

    /// The declared parameter argument `index` takes, when it lies in a group.
    pub(crate) const fn parameter(self, index: usize) -> Option<usize> {
        if index < self.from {
            None
        } else {
            Some(self.from + (index - self.from) % self.size)
        }
    }
}

/// What a formula's compiler knows of an operand from its syntax alone, in the form a
/// [`FunctionCallTarget::check_entry`] reads. Parentheses are transparent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OperandForm {
    /// An omitted argument: `COUNTIF(,1)`.
    Missing,
    /// A number, text, logical or error literal.
    Constant,
    /// An array constant: `{1,2}`.
    ArrayConstant,
    /// A reference, or an operation that gives one: a range, an intersection, a union, a spill
    /// (`A1#`), a trimmed range, an implicit intersection (`@A1:A2`); also `#REF!`, a reference
    /// that refers to nothing (`=COUNTBLANK(#REF!)` enters, `=COUNTBLANK(#N/A)` does not).
    Reference,
    /// A call of a catalogue entry.
    Call(FunctionId),
    /// A LAMBDA.
    Lambda,
    /// A LET.
    Let,
    /// A name: a LET name, a LAMBDA parameter, a defined name or a name nothing defines.
    Name,
    /// A call of a value, or of a name that is no catalogue entry: `f(1)`, `LAMBDA(x,x)(1)`.
    Invocation,
    /// An operation of values: arithmetic, comparison, concatenation, a sign, a percentage.
    Expression,
}

impl OperandForm {
    /// Whether Excel's entry takes this operand where a parameter requires a reference
    /// ([`EntryClass::Reference`]), and before a call postfix.
    pub fn fills_reference_place(self) -> bool {
        match self {
            OperandForm::Reference
            | OperandForm::Name
            | OperandForm::Lambda
            | OperandForm::Let
            | OperandForm::Invocation => true,
            OperandForm::Call(id) => catalogue()
                .resolve(id)
                .is_ok_and(|target| target.result_form().may_be_reference_or_callable()),
            OperandForm::Missing
            | OperandForm::Constant
            | OperandForm::ArrayConstant
            | OperandForm::Expression => false,
        }
    }
}

/// Why Excel's entry refuses a call of an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntryRule {
    /// Fewer arguments than the arity admits: `=IRR()`.
    TooFewArguments,
    /// More arguments than the arity admits: `=PI(1)`.
    TooManyArguments,
    /// A repeating group left incomplete: `=IFS(TRUE,1,FALSE)`, `=SUMIFS(B1:B3,B1:B3,1,B1:B3)`.
    IncompleteGroup,
    /// Argument `index` (from 0) fills a reference place with an operand that cannot give a
    /// reference: `=COUNTBLANK(2)`, `=COUNTBLANK({"",1})`.
    ReferenceRequired { index: usize },
    /// The call's result is called, and the entry gives a value: `=IFERROR(#N/A,LAMBDA(x,x))(3)`.
    ResultNotCallable,
}

impl fmt::Display for EntryRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntryRule::TooFewArguments => f.write_str("too few arguments"),
            EntryRule::TooManyArguments => f.write_str("too many arguments"),
            EntryRule::IncompleteGroup => f.write_str("an incomplete group of arguments"),
            EntryRule::ReferenceRequired { index } => {
                write!(f, "argument {} requires a reference", index + 1)
            }
            EntryRule::ResultNotCallable => f.write_str("its result cannot be called"),
        }
    }
}

impl FunctionCallTarget<'_> {
    /// What a call of this entry may give.
    pub fn result_form(self) -> ResultForm {
        self.entry.result
    }

    /// The repeating groups of this entry's arguments, as `(from, size, tail)`
    /// ([`ArgumentGroups`]).
    pub fn argument_groups(self) -> Option<(usize, usize, usize)> {
        self.entry
            .groups
            .map(|groups| (groups.from, groups.size, groups.tail))
    }

    /// What Excel's entry accepts in argument `index`, or `None` when the arity admits no
    /// argument there.
    pub fn entry_class(self, index: usize) -> Option<EntryClass> {
        (index < self.entry.arity.max).then(|| self.entry.entry_class(index))
    }

    /// Whether Excel's entry takes a call of this entry with these operands, in order: the
    /// arity, the repeating groups, then each reference place, the first refused one named.
    pub fn check_entry(self, operands: &[OperandForm]) -> Result<(), EntryRule> {
        let entry = self.entry;
        if operands.len() < entry.arity.min {
            return Err(EntryRule::TooFewArguments);
        }
        if operands.len() > entry.arity.max {
            return Err(EntryRule::TooManyArguments);
        }
        if entry
            .groups
            .is_some_and(|groups| groups.incomplete(operands.len()))
        {
            return Err(EntryRule::IncompleteGroup);
        }
        for (index, operand) in operands.iter().enumerate() {
            if entry.entry_class(index) == EntryClass::Reference
                && !operand.fills_reference_place()
            {
                return Err(EntryRule::ReferenceRequired { index });
            }
        }
        Ok(())
    }

    /// Whether Excel's entry takes a call postfix after a call of this entry.
    pub fn check_called(self) -> Result<(), EntryRule> {
        if self.entry.result.may_be_reference_or_callable() {
            Ok(())
        } else {
            Err(EntryRule::ResultNotCallable)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Notation;

    fn target(spelling: &str) -> FunctionCallTarget<'static> {
        catalogue()
            .resolve_surface(Notation::Call, spelling)
            .expect("a catalogue entry")
    }

    fn call(spelling: &str) -> OperandForm {
        OperandForm::Call(target(spelling).id())
    }

    /// Native and browser: the measured COUNTBLANK cases of the module documentation.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn countblank_takes_only_what_may_give_a_reference() {
        use OperandForm as O;
        let countblank = target("COUNTBLANK");
        for refused in [
            O::Constant,
            O::ArrayConstant,
            O::Expression,
            O::Missing,
            call("SUM"),
            call("TRUE"),
            call("IFERROR"),
            call("IFNA"),
        ] {
            assert_eq!(
                countblank.check_entry(&[refused]),
                Err(EntryRule::ReferenceRequired { index: 0 }),
                "{refused:?}"
            );
        }
        for accepted in [
            O::Reference,
            O::Name,
            O::Lambda,
            O::Let,
            O::Invocation,
            call("INDEX"),
            call("OFFSET"),
            call("INDIRECT"),
            call("IF"),
            call("CHOOSE"),
            call("XLOOKUP"),
            call("SWITCH"),
            call("IFS"),
        ] {
            assert_eq!(countblank.check_entry(&[accepted]), Ok(()), "{accepted:?}");
        }
        assert_eq!(countblank.check_entry(&[]), Err(EntryRule::TooFewArguments));
        assert_eq!(
            countblank.check_entry(&[O::Reference, O::Reference]),
            Err(EntryRule::TooManyArguments)
        );
    }

    /// Native and browser: the measured groups (dc-mx3x): IFS and COUNTIFS take even counts,
    /// SUMIFS, AVERAGEIFS, MAXIFS and MINIFS odd counts from 3, SORTBY 2, 3, 5, 7, and SWITCH
    /// every count from 3.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn repeating_groups_must_be_complete() {
        let taken = |spelling: &str, count: usize| {
            target(spelling)
                .check_entry(&vec![OperandForm::Reference; count])
                .is_ok()
        };
        let counts = |spelling: &str| -> Vec<usize> {
            (0..9).filter(|&count| taken(spelling, count)).collect()
        };
        assert_eq!(counts("IFS"), [2, 4, 6, 8]);
        assert_eq!(counts("COUNTIFS"), [2, 4, 6, 8]);
        for spelling in ["SUMIFS", "AVERAGEIFS", "MAXIFS", "MINIFS"] {
            assert_eq!(counts(spelling), [3, 5, 7], "{spelling}");
        }
        assert_eq!(counts("SORTBY"), [2, 3, 5, 7]);
        assert_eq!(counts("SWITCH"), [3, 4, 5, 6, 7, 8]);
        assert_eq!(counts("CHOOSE"), [2, 3, 4, 5, 6, 7, 8]);
    }

    /// Native and browser: a constant in each measured reference place is refused, and a
    /// reference there enters (dc-mx3x's census of every catalogue entry: each argument in turn
    /// `2`, the others `B1`).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_measured_reference_places() {
        let places: &[(&str, usize, &[usize])] = &[
            ("AGGREGATE", 5, &[4]),
            ("AREAS", 1, &[0]),
            ("AVERAGEIF", 3, &[0, 2]),
            ("AVERAGEIFS", 5, &[0, 1, 3]),
            ("CELL", 2, &[1]),
            ("COLUMN", 1, &[0]),
            ("COUNTBLANK", 1, &[0]),
            ("COUNTIF", 2, &[0]),
            ("COUNTIFS", 4, &[0, 2]),
            ("DAVERAGE", 3, &[0]),
            ("DCOUNT", 3, &[0]),
            ("DCOUNTA", 3, &[0]),
            ("DGET", 3, &[0]),
            ("DMAX", 3, &[0]),
            ("DMIN", 3, &[0]),
            ("DPRODUCT", 3, &[0]),
            ("DSTDEV", 3, &[0]),
            ("DSTDEVP", 3, &[0]),
            ("DSUM", 3, &[0]),
            ("DVAR", 3, &[0]),
            ("DVARP", 3, &[0]),
            ("FORMULATEXT", 1, &[0]),
            ("ISFORMULA", 1, &[0]),
            ("MAXIFS", 5, &[0, 1, 3]),
            ("MINIFS", 5, &[0, 1, 3]),
            ("OFFSET", 5, &[0]),
            ("PHONETIC", 1, &[0]),
            ("RANK", 3, &[1]),
            ("RANK.AVG", 3, &[1]),
            ("RANK.EQ", 3, &[1]),
            ("ROW", 1, &[0]),
            ("SUBTOTAL", 4, &[1, 2, 3]),
            ("SUMIF", 3, &[0, 2]),
            ("SUMIFS", 5, &[0, 1, 3]),
        ];
        for &(spelling, count, measured) in places {
            let target = target(spelling);
            let refused: Vec<usize> = (0..count)
                .filter(|&index| {
                    let mut operands = vec![OperandForm::Reference; count];
                    operands[index] = OperandForm::Constant;
                    target.check_entry(&operands).is_err()
                })
                .collect();
            assert_eq!(refused, measured, "{spelling}");
            assert_eq!(
                target.check_entry(&vec![OperandForm::Reference; count]),
                Ok(())
            );
        }
        // Every other entry takes a constant everywhere.
        let named: Vec<&str> = places.iter().map(|place| place.0).collect();
        for target in catalogue().targets() {
            if named.contains(&target.spelling()) || target.notation() != Notation::Call {
                continue;
            }
            let count = target.arity().min.max(1).min(target.arity().max);
            for index in 0..count {
                assert_ne!(
                    target.entry_class(index),
                    Some(EntryClass::Reference),
                    "{} argument {index}",
                    target.spelling()
                );
            }
        }
    }

    /// Native and browser: the twelve entries whose calls fill a reference place and may be
    /// called (dc-mx3x: of all 509 measured, exactly these took both `=COUNTBLANK(F(B1,...))` and
    /// `=F(B1,...)(3)`; every other entry took neither).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_measured_result_forms() {
        let measured = [
            "CHOOSE", "DROP", "IF", "IFS", "INDEX", "INDIRECT", "OFFSET", "REDUCE", "SWITCH",
            "TAKE", "TRIMRANGE", "XLOOKUP",
        ];
        let found: Vec<&str> = catalogue()
            .targets()
            .filter(|target| target.result_form().may_be_reference_or_callable())
            .map(|target| target.spelling())
            .collect();
        assert_eq!(found, {
            let mut sorted: Vec<&str> = measured.to_vec();
            sorted.sort_by_key(|spelling| target(spelling).id());
            sorted
        });
        assert_eq!(target("IFERROR").check_called(), Err(EntryRule::ResultNotCallable));
        assert_eq!(target("IF").check_called(), Ok(()));
    }
}
