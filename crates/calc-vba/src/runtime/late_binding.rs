//! Late-bound COM activation in a project's own source.
//!
//! Owner rule: early-bound COM through imported type libraries first, never late binding as a
//! start. `New X` of a class an imported type library declares compiles to an activation by the
//! class's ProgID, which the runtime allows; an explicit `CreateObject(...)` or `GetObject(...)`
//! is late binding. The compiled bundle makes the two the same call, so the runtime learns the
//! second kind from the project's source: the ProgIDs it passes as literals are refused, and when
//! one of those calls passes anything else, every activation by ProgID is.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LateBinding {
    refuse_all: bool,
    prog_ids: Vec<String>,
}

#[derive(Debug, PartialEq)]
enum Token {
    Word(String),
    Text(String),
    Mark(char),
}

impl LateBinding {
    /// The late-bound activations in `sources` (the text of VBA modules).
    pub(crate) fn of<'a>(sources: impl IntoIterator<Item = &'a str>) -> Self {
        let mut found = Self::default();
        for source in sources {
            for line in source.lines() {
                found.scan(&tokens(line));
            }
        }
        found
    }

    /// Whether the activation of `prog_id` is a late-bound one.
    pub(crate) fn refuses(&self, prog_id: &str) -> bool {
        self.refuse_all
            || self
                .prog_ids
                .iter()
                .any(|p| p.eq_ignore_ascii_case(prog_id))
    }

    fn scan(&mut self, line: &[Token]) {
        for (at, token) in line.iter().enumerate() {
            let Token::Word(word) = token else { continue };
            let class_argument = if word.eq_ignore_ascii_case("CreateObject") {
                0
            } else if word.eq_ignore_ascii_case("GetObject") {
                1
            } else {
                continue;
            };
            // A member of something else (`x.CreateObject`) is not the VBA function.
            if at > 0 && line[at - 1] == Token::Mark('.') {
                continue;
            }
            if line.get(at + 1) != Some(&Token::Mark('(')) {
                continue;
            }
            let mut depth = 0usize;
            let mut arguments: Vec<Vec<&Token>> = vec![Vec::new()];
            for token in &line[at + 2..] {
                match token {
                    Token::Mark('(') => depth += 1,
                    Token::Mark(')') if depth == 0 => break,
                    Token::Mark(')') => depth -= 1,
                    Token::Mark(',') if depth == 0 => {
                        arguments.push(Vec::new());
                        continue;
                    }
                    _ => {}
                }
                arguments.last_mut().expect("an argument").push(token);
            }
            match arguments.get(class_argument).map(Vec::as_slice) {
                Some([Token::Text(class)]) if !class.is_empty() => {
                    self.prog_ids.push(class.clone())
                }
                _ => self.refuse_all = true,
            }
        }
    }
}

/// The tokens of one line of VBA: words, string literals (`""` inside is one quote), single marks.
/// A comment (`'`) ends the line.
fn tokens(line: &str) -> Vec<Token> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            break;
        } else if c == '"' {
            let mut text = String::new();
            i += 1;
            while i < chars.len() {
                if chars[i] == '"' {
                    if chars.get(i + 1) == Some(&'"') {
                        text.push('"');
                        i += 2;
                        continue;
                    }
                    break;
                }
                text.push(chars[i]);
                i += 1;
            }
            out.push(Token::Text(text));
        } else if c.is_alphanumeric() || c == '_' {
            let mut word = String::new();
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                word.push(chars[i]);
                i += 1;
            }
            out.push(Token::Word(word));
            continue;
        } else if !c.is_whitespace() {
            out.push(Token::Mark(c));
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn explicit_activation_is_found_and_new_is_not() {
        let found = LateBinding::of([
            "Dim d As New Scripting.Dictionary\nSet e = New Scripting.Dictionary\n",
            "Set x = CreateObject(\"Scripting.FileSystemObject\") ' CreateObject(\"No.Comment\")\n",
            "s = \"CreateObject(\"\"Not.Code\"\")\"\nSet y = Application.CreateObject(\"Not.Vba\")\n",
            "Set z = GetObject(\"\", \"Excel.Application\")\n",
        ]);
        assert!(!found.refuse_all, "{found:?}");
        assert!(found.refuses("scripting.filesystemobject") && found.refuses("Excel.Application"));
        assert!(!found.refuses("Scripting.Dictionary"));
        assert!(
            !found.refuses("No.Comment") && !found.refuses("Not.Code") && !found.refuses("Not.Vba")
        );
        // A ProgID that is not a literal could be any class: every activation is refused.
        let any = LateBinding::of(["Set x = CreateObject(name)\n"]);
        assert!(any.refuses("Scripting.Dictionary"));
        let path = LateBinding::of(["Set x = GetObject(\"C:\\a.xlsx\")\n"]);
        assert!(path.refuses("Scripting.Dictionary"));
    }
}
