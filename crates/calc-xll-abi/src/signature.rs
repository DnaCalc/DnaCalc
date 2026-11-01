//! xlfRegister Form 1 type strings; parsing does not promise invocation support.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XllType {
    Boolean,
    Double,
    UShort,
    Short,
    Int,
    DoublePointer,
    BooleanPointer,
    ShortPointer,
    IntPointer,
    CString,
    CountedString,
    CStringInPlace,
    CountedStringInPlace,
    WideString,
    WideCountedString,
    WideStringInPlace,
    WideCountedStringInPlace,
    Fp,
    Fp12,
    Array,
    Array12,
    Oper,
    OperReference,
    Oper12,
    Oper12Reference,
    AsyncHandle,
    Void,
    InPlace(u8),
}

impl XllType {
    pub fn is_pointer(self) -> bool {
        !matches!(
            self,
            Self::Boolean
                | Self::Double
                | Self::UShort
                | Self::Short
                | Self::Int
                | Self::Void
                | Self::InPlace(_)
        )
    }
    pub fn allows_reference(self) -> bool {
        matches!(self, Self::OperReference | Self::Oper12Reference)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XllSignature {
    pub result: XllType,
    pub arguments: Vec<XllType>,
    pub volatile: bool,
    pub macro_equivalent: bool,
    pub thread_safe: bool,
    pub cluster_safe: bool,
    pub asynchronous: bool,
}

impl XllSignature {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut codes = Vec::new();
        let mut volatile = false;
        let mut macro_equivalent = false;
        let mut thread_safe = false;
        let mut cluster_safe = false;
        let mut flags = false;
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            let flag = match ch {
                '!' => Some(&mut volatile),
                '#' => Some(&mut macro_equivalent),
                '$' => Some(&mut thread_safe),
                '&' => Some(&mut cluster_safe),
                _ => None,
            };
            if let Some(flag) = flag {
                if *flag {
                    return Err(format!("repeated registration flag {ch}"));
                }
                *flag = true;
                flags = true;
                continue;
            }
            if flags {
                return Err("a type follows registration flags".into());
            }
            let wide = chars.peek() == Some(&'%');
            if wide {
                chars.next();
            }
            let code = match (ch, wide) {
                ('A', false) => XllType::Boolean,
                ('B', false) => XllType::Double,
                ('H', false) => XllType::UShort,
                ('I', false) => XllType::Short,
                ('J', false) => XllType::Int,
                ('E', false) => XllType::DoublePointer,
                ('L', false) => XllType::BooleanPointer,
                ('M', false) => XllType::ShortPointer,
                ('N', false) => XllType::IntPointer,
                ('C', false) => XllType::CString,
                ('D', false) => XllType::CountedString,
                ('F', false) => XllType::CStringInPlace,
                ('G', false) => XllType::CountedStringInPlace,
                ('C', true) => XllType::WideString,
                ('D', true) => XllType::WideCountedString,
                ('F', true) => XllType::WideStringInPlace,
                ('G', true) => XllType::WideCountedStringInPlace,
                ('K', false) => XllType::Fp,
                ('K', true) => XllType::Fp12,
                ('O', false) => XllType::Array,
                ('O', true) => XllType::Array12,
                ('P', false) => XllType::Oper,
                ('R', false) => XllType::OperReference,
                ('Q', false) => XllType::Oper12,
                ('U', false) => XllType::Oper12Reference,
                ('X', false) => XllType::AsyncHandle,
                ('>', false) => XllType::Void,
                ('1'..='9', false) => XllType::InPlace(ch as u8 - b'0'),
                _ => {
                    return Err(format!(
                        "unknown registration type {ch}{}",
                        if wide { "%" } else { "" }
                    ));
                }
            };
            codes.push(code);
        }
        if codes.is_empty() {
            return Err("empty registration type string".into());
        }
        let result = codes.remove(0);
        if codes.len() > 255 {
            return Err("more than 255 function arguments".into());
        }
        if codes
            .iter()
            .any(|c| matches!(c, XllType::Void | XllType::InPlace(_)))
        {
            return Err("void/in-place marker used as an argument type".into());
        }
        if matches!(
            result,
            XllType::AsyncHandle | XllType::Array | XllType::Array12
        ) {
            return Err("argument-only type used as a return type".into());
        }
        let handles = codes.iter().filter(|c| **c == XllType::AsyncHandle).count();
        let asynchronous = handles != 0;
        if asynchronous && (handles != 1 || result != XllType::Void || cluster_safe) {
            return Err("async requires one X argument, > return and no & flag".into());
        }
        if macro_equivalent && thread_safe {
            return Err("# and $ are mutually exclusive".into());
        }
        if let XllType::InPlace(n) = result {
            if !codes
                .get(usize::from(n) - 1)
                .is_some_and(|c| c.is_pointer() && *c != XllType::AsyncHandle)
            {
                return Err("in-place return does not name a pointer argument".into());
            }
        }
        volatile |= macro_equivalent && codes.iter().any(|c| c.allows_reference());
        Ok(Self {
            result,
            arguments: codes,
            volatile,
            macro_equivalent,
            thread_safe,
            cluster_safe,
            asynchronous,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Native-only: registration signatures belong to the native-only XLL adapter.
    #[test]
    fn x0_observed_signatures_and_invalid_combinations() {
        for signature in [
            "QD%", "QE$", "QU#", "Q#", "Q", ">EX", ">B", "BBB", "QK%", "1F%MM",
        ] {
            XllSignature::parse(signature).unwrap();
        }
        let reference = XllSignature::parse("QU#").unwrap();
        assert!(reference.volatile && reference.macro_equivalent);
        assert_eq!(reference.arguments, [XllType::Oper12Reference]);
        for signature in [
            "", "Q#$", "QX", ">XX", ">EX&", "Q%", "Q!B", "2B", "Q>", "X", "O%",
        ] {
            assert!(XllSignature::parse(signature).is_err(), "{signature}");
        }
        assert!(XllSignature::parse(&format!("Q{}", "Q".repeat(255))).is_ok());
        assert!(XllSignature::parse(&format!("Q{}", "Q".repeat(256))).is_err());
    }
}
