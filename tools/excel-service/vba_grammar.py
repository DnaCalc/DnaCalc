"""The positive grammar for Touchstone VBA probe source (dc-gqzuw).

Only the statement forms, expression forms and member chains below are accepted; every other token or
form is refused. The rule table is vba-probe-policy.json and engines/excel/VbaGrammar.cs reads the same
file and implements the same algorithm; keep the two in step (the tests run one list of accepted and
refused sources through both).

Cell writes are data: a value written to a cell through .Value or .Value2 must be a number, a boolean, a
string literal that does not start with = + - or @, or a variable, call result or concatenation that is
provably the same (parameters hold case arguments, which the case check keeps to data). A value read
from a cell, a dictionary or a string function is never written back, because a worksheet formula can
produce text that starts with =.

Names reach only ThisWorkbook (and Me in a document module), the listed members of the listed object
kinds, and local variables. There is no Application, no Workbooks, no unqualified Range or Cells, no
Sheets.Add or Worksheets.Add (a sheet is reached only through an indexed Worksheets or Sheets call).
"""
import json
import re
from pathlib import Path

RULES = json.loads(Path(__file__).with_name("vba-probe-policy.json").read_text(encoding="ascii"))
LEX = re.compile(RULES["lex"])
IDENTIFIER = re.compile(RULES["identifier"])
MODULE_NAME = re.compile(RULES["module_name"])
SHEET_CODE_NAME = re.compile(r"sheet[1-9][0-9]{0,1}")
KEYWORDS = set(RULES["keywords"].split())
VALUE_TYPES = set(RULES["value_types"].split())
NUMERIC_TYPES = set(RULES["numeric_types"].split())
OBJECT_TYPES = {"range": "range", "worksheet": "sheet", "workbook": "wb", "object": "pending"}
EVENTS = set(RULES["event_names"].split())
CONSTANTS = set(RULES["constants"].split())
BUILTINS = {}
for _row in RULES["builtins"]:
    _name, _low, _high, _safe = _row.split()
    BUILTINS[_name] = (int(_low), int(_high), _safe)
MEMBERS = {}
for _row in RULES["members"]:
    _kind, _name, _ret, _low, _high, _flags = _row.split()
    MEMBERS.setdefault(_kind, {})[_name] = (_ret, int(_low), int(_high), _flags)
RESERVED = (set(RULES["reserved"].split()) | KEYWORDS | VALUE_TYPES | set(OBJECT_TYPES) | CONSTANTS | set(BUILTINS))
CLASSES = {name.lower(): kind for name, kind in zip(RULES["classes"], ("dict", "regexp"))}
OBJECT_KINDS = {"wb", "sheet", "range", "dict", "regexp", "err"}
FORMULA_START = RULES["formula_chars"]
# Taint classes of a value (dc-5qqoz): TEXT is a string that cannot start with = + - or @; NUMBER is a number
# or boolean (it may be negative, so its text may start with -); NUMTEXT is a string made from a number;
# BAD is anything else. A cell write takes TEXT or NUMBER; a concatenation's leading operand must be TEXT.
# rw17 (dc-erfm3): SAFE is a string that is also never empty (a non-empty literal, TypeName), the only kind that
# may lead a concatenation; TEXT may be empty at run time ("", vbNullString, Empty, Null, Left(s, 0), an
# unassigned variable), so a number after it could lead the result.
SAFE_TEXT, TEXT, NUMBER, NUMTEXT, BAD = 0, 1, 2, 3, 4
SAFE, MAYBE, NUM, UNSAFE = ("safe",), ("maybe",), ("num",), ("unsafe",)
ARITHMETIC = {"-", "*", "/", "\\", "^", "mod", "=", "<", ">", "<=", ">=", "<>", "and", "or", "xor", "eqv", "imp", "is", "like"}
BINARY = ARITHMETIC | {"+", "&"}


def escape(text):
    return text.encode("ascii", "backslashreplace").decode("ascii")


def refuse(what, line):
    shown = escape(line if len(line) <= 100 else line[:100] + "...")
    raise ValueError(f"VBA construct refused: {escape(what)} in: {shown}")


class Token:
    __slots__ = ("kind", "text", "low")

    def __init__(self, kind, text):
        self.kind, self.text, self.low = kind, text, text.lower()


def logical_lines(source):
    """The source as (text, tokens) per logical line: comments dropped, ` _` continuations joined."""
    if "\r" in source.replace("\r\n", ""):
        raise ValueError("VBA construct refused: a bare carriage return")
    lines, tokens, text, pending = [], [], "", False
    for physical in source.replace("\r\n", "\n").split("\n"):
        at, after_space, pending = 0, False, False
        while at < len(physical):
            match = LEX.match(physical, at)
            if not match:
                refuse(physical[at], physical)
            at = match.end()
            kind = match.lastgroup
            if kind == "cont":
                if not after_space:
                    refuse("_", physical)
                pending = True
                break
            after_space = kind == "ws"
            if kind not in ("ws", "comment"):
                tokens.append(Token(kind, match.group()))
        text += physical + " "
        if not pending:
            if tokens:
                lines.append((text.strip(), tokens))
            tokens, text = [], ""
    if pending:
        refuse("_", text)
    return lines


class Var:
    def __init__(self, key, kind="val", numeric=False, param=False, array=False, const=False):
        self.key, self.kind, self.numeric, self.param, self.array, self.const = key, kind, numeric, param, array, const


class Proc:
    def __init__(self, module, name, is_function, public, params, line):
        self.module, self.name, self.is_function, self.public = module, name, is_function, public
        self.params, self.line = params, line
        self.key = f"{module}.{name}"
        self.body = []


class Case:
    """One case's modules, checked together: procedures call across modules and taint flows between them."""

    def __init__(self, modules):
        self.modules = modules
        self.names = {name.lower() for name, _, _ in modules}
        self.kinds = {}           # module -> "standard" | "document"
        self.procs = {}           # "module.proc" -> Proc
        self.module_vars = {}     # module -> {name: Var}
        self.flows = []           # (key, node): assignments and argument bindings
        self.writes = []          # (node, line): values written to cells
        self.numeric = set()      # variables whose declared type holds only numbers and booleans
        self.params = set()       # procedure parameters: a case argument may be a (negative) number
        self.strings = set()      # variables declared As String: a number assigned to one becomes number text

    def declared(self, token, line):
        if token.kind != "id" or not IDENTIFIER.fullmatch(token.text) or token.low in RESERVED \
                or token.low in self.names:
            refuse(token.text, line)
        return token.low

    def run(self):
        for name, kind, source in self.modules:
            self.scan(name.lower(), kind, source)
        for proc in list(self.procs.values()):
            Body(self, proc).run()
        classes = {key: NUMBER for key in self.params | self.numeric}
        for key in self.params & self.strings:
            classes[key] = NUMTEXT          # a number argument becomes number text in an As String parameter
        changed = True
        while changed:
            changed = False
            for key, node in self.flows:
                new = max(classes.get(key, TEXT), self.classify(node, classes))
                if key in self.numeric:
                    new = NUMBER
                elif key in self.strings and new == NUMBER:
                    new = NUMTEXT
                if new != classes.get(key, TEXT):
                    classes[key] = new
                    changed = True
        for node, line in self.writes:
            if self.classify(node, classes) > NUMBER:
                refuse("a cell value that is not plainly data", line)
        self.entry_points = {p.key for p in self.procs.values() if p.public and self.kinds[p.module] == "standard"}
        return self.entry_points

    def classify(self, node, classes):
        tag = node[0]
        if tag == "safe":
            return SAFE_TEXT
        if tag == "maybe":
            return TEXT
        if tag == "num":
            return NUMBER
        if tag == "unsafe":
            return BAD
        if tag == "var":
            return classes.get(node[1], TEXT)
        found = [self.classify(n, classes) for n in node[1]]
        if BAD in found:
            return BAD
        if tag == "join":                       # Array(...): the elements are written one by one
            return max(found, default=TEXT)
        if tag == "str":                        # Left, CStr, LCase, UCase: the text of the first argument
            if found[0] == NUMBER:
                return NUMTEXT
            if found[0] > NUMBER:
                return found[0]
            return TEXT if node[2] == "left" else found[0]
        ops = node[2]                           # "cat": operands joined by the operators between them
        if "+" in ops:                          # + adds a number, a boolean or a numeric string: only numbers
            return NUMBER if all(op not in ("&",) for op in ops) and all(c == NUMBER for c in found) else BAD
        if any(op not in ("&",) for op in ops):
            return BAD if "&" in ops else NUMBER
        if any(c > TEXT for c in found) and found[0] > SAFE_TEXT:
            return BAD
        if found[0] == SAFE_TEXT:
            return SAFE_TEXT
        return TEXT if all(c <= TEXT for c in found) else BAD

    def scan(self, module, kind, source):
        self.kinds[module] = kind
        self.module_vars[module] = {}
        current, explicit = None, False
        for text, tokens in logical_lines(source):
            words = [t.low for t in tokens]
            if current is not None:
                if words[:1] == ["end"] and len(words) == 2 and words[1] in ("sub", "function"):
                    if words[1] != ("function" if current.is_function else "sub"):
                        refuse(text, text)
                    current = None
                else:
                    current.body.append((text, tokens))
                continue
            if words == ["option", "explicit"] and not explicit:
                explicit = True
                continue
            header = self.header(module, kind, tokens, text)
            if header is not None:
                if header.key in self.procs:
                    refuse(header.name, text)
                self.procs[header.key] = current = header
                continue
            self.module_declaration(module, tokens, text)
        if current is not None:
            refuse("a procedure without End", current.line)

    def header(self, module, kind, tokens, text):
        at, public = 0, True
        if tokens[0].low in ("public", "private"):
            public, at = tokens[0].low == "public", 1
        if at >= len(tokens) or tokens[at].low not in ("sub", "function"):
            return None
        is_function = tokens[at].low == "function"
        if at + 1 >= len(tokens):
            refuse(tokens[at].text, text)
        name = self.declared(tokens[at + 1], text)
        if kind == "document":
            if name not in EVENTS or not name.startswith("workbook_" if module == "thisworkbook" else "worksheet_"):
                refuse(tokens[at + 1].text, text)
        elif name.startswith(("workbook_", "worksheet_")):
            refuse(tokens[at + 1].text, text)
        pos, params = at + 2, []
        if pos < len(tokens) and tokens[pos].text == "(":
            pos += 1
            if pos < len(tokens) and tokens[pos].text == ")":
                pos += 1
            else:
                while True:
                    if pos < len(tokens) and tokens[pos].low in ("byval", "byref"):
                        pos += 1
                    if pos >= len(tokens):
                        refuse(text, text)
                    pname = self.declared(tokens[pos], text)
                    pos += 1
                    ptype = "variant"
                    if pos + 1 < len(tokens) and tokens[pos].low == "as":
                        ptype = tokens[pos + 1].low
                        if ptype not in VALUE_TYPES and ptype not in OBJECT_TYPES:
                            refuse(tokens[pos + 1].text, text)
                        pos += 2
                    if pname in [p[0] for p in params]:
                        refuse(pname, text)
                    params.append((pname, ptype))
                    if pos < len(tokens) and tokens[pos].text == ",":
                        pos += 1
                    elif pos < len(tokens) and tokens[pos].text == ")":
                        pos += 1
                        break
                    else:
                        refuse(tokens[min(pos, len(tokens) - 1)].text, text)
        if pos < len(tokens) and tokens[pos].low == "as":
            if not is_function or pos + 1 >= len(tokens) or tokens[pos + 1].low not in VALUE_TYPES:
                refuse(tokens[pos].text, text)
            if tokens[pos + 1].low in NUMERIC_TYPES:
                self.numeric.add(f"{module}.{name}.{name}")
            elif tokens[pos + 1].low == "string":
                self.strings.add(f"{module}.{name}.{name}")
            pos += 2
        if pos != len(tokens):
            refuse(tokens[pos].text, text)
        return Proc(module, name, is_function, public, params, text)

    def module_declaration(self, module, tokens, text):
        if tokens[0].low not in ("private", "public", "dim"):
            refuse(tokens[0].text, text)
        rest = tokens[1:]
        if rest and rest[0].low == "const":
            self.constant(rest[1:], text, self.module_vars[module], f"{module}..")
        else:
            self.variables(rest, text, self.module_vars[module], f"{module}..", allow_objects=False)

    def variables(self, tokens, text, scope, prefix, allow_objects=True):
        pos = 0
        while True:
            if pos >= len(tokens):
                refuse(text, text)
            name = self.declared(tokens[pos], text)
            if name in scope:
                refuse(tokens[pos].text, text)
            pos += 1
            array = False
            if pos < len(tokens) and tokens[pos].text == "(":
                array, pos = True, pos + 1
                while pos < len(tokens) and tokens[pos].text != ")":
                    token = tokens[pos]
                    if not (token.kind == "num" and token.text.isdigit() or token.low == "to" or token.text == ","):
                        refuse(token.text, text)
                    pos += 1
                if pos >= len(tokens):
                    refuse(text, text)
                pos += 1
            vtype = "variant"
            if pos + 1 < len(tokens) and tokens[pos].low == "as":
                vtype = tokens[pos + 1].low
                if vtype not in VALUE_TYPES and not (allow_objects and not array and vtype in OBJECT_TYPES):
                    refuse(tokens[pos + 1].text, text)
                pos += 2
            scope[name] = Var(prefix + name, OBJECT_TYPES.get(vtype, "val"), numeric=vtype in NUMERIC_TYPES, array=array)
            if vtype in NUMERIC_TYPES:
                self.numeric.add(prefix + name)
            if vtype == "string":
                self.strings.add(prefix + name)
            if pos == len(tokens):
                return
            if tokens[pos].text != ",":
                refuse(tokens[pos].text, text)
            pos += 1

    def constant(self, tokens, text, scope, prefix):
        if len(tokens) != 3 or tokens[1].text != "=" or tokens[2].kind not in ("num", "str", "hex"):
            refuse(text, text)
        name = self.declared(tokens[0], text)
        if name in scope:
            refuse(tokens[0].text, text)
        scope[name] = Var(prefix + name, const=True)
        self.flows.append((prefix + name, self.literal(tokens[2])))

    @staticmethod
    def literal(token):
        if token.kind == "str":
            body = token.text[1:-1].replace('""', '"')
            if not body:
                return MAYBE
            return UNSAFE if body[0] in FORMULA_START else SAFE
        return NUM          # a number or a hex literal (&H8000 is -32768)


class Body:
    def __init__(self, case, proc):
        self.case, self.proc, self.module = case, proc, proc.module
        self.prefix = proc.key + "."
        self.scope = {}
        for pname, ptype in proc.params:
            kind = OBJECT_TYPES.get(ptype, "val")
            self.scope[pname] = Var(self.prefix + pname, "val" if kind == "pending" else kind,
                                    numeric=ptype in NUMERIC_TYPES, param=True)
            if ptype in NUMERIC_TYPES:
                case.numeric.add(self.prefix + pname)
            if ptype == "string":
                case.strings.add(self.prefix + pname)
            case.params.add(self.prefix + pname)
        if proc.is_function:
            self.scope[proc.name] = Var(self.prefix + proc.name)
        self.stack, self.labels, self.jumps = [], set(), []

    def run(self):
        for text, tokens in self.proc.body:
            self.logical(text, tokens)
        if self.stack:
            refuse("an unclosed block", self.proc.line)
        for label, text in self.jumps:
            if label not in self.labels:
                refuse(label, text)

    def logical(self, text, tokens):
        # A leading `name:` or `number:` is a label; elsewhere a colon separates statements.
        if len(tokens) >= 2 and tokens[1].text == ":" and (
                tokens[0].kind == "num" and tokens[0].text.isdigit()
                or tokens[0].kind == "id" and tokens[0].low not in RESERVED):
            if tokens[0].low in self.labels:
                refuse(tokens[0].text, text)
            self.labels.add(tokens[0].low)
            tokens = tokens[2:]
        statement = []
        for token in tokens + [Token("op", ":")]:
            if token.kind == "op" and token.text == ":":
                if statement:
                    Statement(self, text, statement).run()
                statement = []
            else:
                statement.append(token)


BINARY_SYMBOLS = {"+", "-", "*", "/", "\\", "^", "&", "=", "<", ">", "<=", ">=", "<>"}
BINARY_WORDS = {"mod", "and", "or", "xor", "eqv", "imp", "is", "like"}
BLOCK_WORDS = {"if", "else", "elseif", "end", "for", "next", "do", "loop", "while", "wend", "select", "case",
               "dim", "const"}


class Statement:
    def __init__(self, body, text, tokens):
        self.body, self.case, self.text, self.tokens, self.at = body, body.case, text, tokens, 0

    def peek(self, ahead=0):
        i = self.at + ahead
        return self.tokens[i] if i < len(self.tokens) else Token("end", "")

    def take(self):
        token = self.peek()
        if token.kind == "end":
            refuse("the end of the statement", self.text)
        self.at += 1
        return token

    def expect(self, text):
        token = self.take()
        if token.low != text:
            refuse(token.text, self.text)

    def done(self):
        if self.at != len(self.tokens):
            refuse(self.peek().text, self.text)

    def fail(self, token=None):
        refuse((token or self.peek()).text or self.text, self.text)

    def scope_var(self, token):
        if token.kind != "id":
            return None
        return self.body.scope.get(token.low) or self.case.module_vars[self.body.module].get(token.low)

    def close(self, what, keep=False):
        stack = self.body.stack
        if not stack or stack[-1] != what:
            self.fail()
        if not keep:
            stack.pop()

    # --- statements -----------------------------------------------------------------------------------
    def run(self):
        first = self.peek()
        word = first.low
        stack = self.body.stack
        if first.kind != "id":
            self.fail()
        if word == "dim":
            self.take()
            self.case.variables(self.tokens[1:], self.text, self.body.scope, self.body.prefix)
            return
        if word == "const":
            self.take()
            self.case.constant(self.tokens[1:], self.text, self.body.scope, self.body.prefix)
            return
        if word == "set":
            return self.set_statement()
        if word == "if":
            return self.if_statement()
        if word in ("elseif", "else"):
            self.take()
            self.close("if", keep=True)
            if word == "elseif":
                self.expression()
                self.expect("then")
            self.done()
            return
        if word == "end":
            self.take()
            what = self.take()
            if what.low not in ("if", "select"):
                self.fail(what)
            self.close(what.low)
            self.done()
            return
        if word == "for":
            return self.for_statement()
        if word == "next":
            self.take()
            self.close("for")
            if self.peek().kind == "id":
                self.take()
            return self.done()
        if word in ("do", "loop"):
            self.take()
            if word == "do":
                stack.append("do")
            else:
                self.close("do")
            if self.peek().low in ("while", "until"):
                self.take()
                self.expression()
            return self.done()
        if word == "while":
            self.take()
            self.expression()
            stack.append("while")
            return self.done()
        if word == "wend":
            self.take()
            self.close("while")
            return self.done()
        if word == "select":
            self.take()
            self.expect("case")
            self.expression()
            stack.append("select")
            return self.done()
        if word == "case":
            self.take()
            self.close("select", keep=True)
            if self.peek().low == "else":
                self.take()
            else:
                while True:
                    if self.peek().low == "is":
                        self.take()
                        if self.take().text not in ("=", "<", ">", "<=", ">=", "<>"):
                            self.fail()
                        self.expression()
                    else:
                        self.expression()
                        if self.peek().low == "to":
                            self.take()
                            self.expression()
                    if self.peek().text != ",":
                        break
                    self.take()
            return self.done()
        if word == "exit":
            self.take()
            what = self.take().low
            function = self.body.proc.is_function
            if not (what == "function" and function or what == "sub" and not function or what in ("for", "do") and what in stack):
                self.fail(first)
            return self.done()
        if word == "on":
            self.take()
            self.expect("error")
            if self.peek().low == "resume":
                self.take()
                self.expect("next")
            else:
                self.expect("goto")
                target = self.take()
                if target.text != "0":
                    self.jump(target)
            return self.done()
        if word == "resume":
            self.take()
            self.expect("next")
            return self.done()
        if word == "goto":
            self.take()
            self.jump(self.take())
            return self.done()
        if word == "call":
            self.take()
            _, _, last = self.chain(statement=False)
            if last != "call":
                self.fail(first)
            return self.done()
        self.simple_statement()

    def jump(self, target):
        if not (target.kind == "id" and target.low not in RESERVED or target.kind == "num" and target.text.isdigit()):
            self.fail(target)
        self.body.jumps.append((target.low, self.text))

    def if_statement(self):
        self.take()
        self.expression()
        self.expect("then")
        if self.at == len(self.tokens):
            self.body.stack.append("if")
            return
        # A single-line If: Then statement [Else statement]; neither part is a block statement.
        parts = [[]]
        for token in self.tokens[self.at:]:
            if token.kind == "id" and token.low == "else" and len(parts) == 1:
                parts.append([])
            else:
                parts[-1].append(token)
        for part in parts:
            if not part or part[0].low in BLOCK_WORDS:
                refuse(part[0].text if part else "Then", self.text)
            Statement(self.body, self.text, part).run()
        self.at = len(self.tokens)

    def for_statement(self):
        self.take()
        each = self.peek().low == "each"
        if each:
            self.take()
        token = self.take()
        var = self.scope_var(token)
        if var is None or var.param or var.array or var.const or var.kind != "val":
            self.fail(token)
        if each:
            self.case.flows.append((var.key, UNSAFE))
            self.expect("in")
            kind, _, _ = self.chain(statement=False)
            if kind != "val":
                self.fail()
        else:
            self.expect("=")
            self.case.flows.append((var.key, NUM))      # a numeric loop: the variable holds numbers
            self.case.flows.append((var.key, self.expression()))
            self.expect("to")
            self.expression()
            if self.peek().low == "step":
                self.take()
                self.expression()
        self.body.stack.append("for")
        self.done()

    def set_statement(self):
        self.take()
        token = self.take()
        var = self.scope_var(token)
        if var is None or var.param or var.array or var.const or var.kind in ("val", "wb") and False:
            self.fail(token)
        self.expect("=")
        if self.peek().low == "nothing" and self.at + 1 == len(self.tokens):
            self.take()
            return
        kind, _, _ = self.chain(statement=False)
        self.done()
        if kind not in OBJECT_KINDS - {"err"} or var.kind not in ("pending", kind):
            self.fail(token)
        var.kind = kind

    def simple_statement(self):
        first = self.peek()
        _, _, last = self.chain(statement=True)
        if self.peek().text != "=":
            if last != "call":
                self.fail(first)
            return self.done()
        self.take()
        if not isinstance(last, tuple):
            self.fail(first)
        if last[0] == "var":
            var = last[1]
            if var.param or var.const or var.kind != "val":
                self.fail(first)
            self.case.flows.append((var.key, self.expression()))
        else:
            _, member, owner = last
            if "s" not in MEMBERS[owner][member][3]:
                self.fail(first)
            if member == "numberformat":
                if len(self.tokens) - self.at != 1 or self.peek().kind != "str":
                    self.fail(first)
                self.take()
            else:
                node = self.expression()
                if member in ("value", "value2") and owner == "range":
                    self.case.writes.append((node, self.text))
        self.done()

    # --- expressions ----------------------------------------------------------------------------------
    def binary(self):
        token = self.peek()
        if token.kind == "op" and token.text in BINARY_SYMBOLS:
            return token.text
        if token.kind == "id" and token.low in BINARY_WORDS:
            return token.low
        return None

    def expression(self):
        nodes, ops = [], []
        while True:
            negated = False
            while self.peek().text == "-" and self.peek().kind == "op" or self.peek().low == "not" and self.peek().kind == "id":
                self.take()
                negated = True
            kind, node, _ = self.chain(statement=False)
            if kind != "val":
                self.fail()
            nodes.append(NUM if negated else node)
            op = self.binary()
            if op is None:
                break
            self.take()
            ops.append(op)
        if len(nodes) == 1:
            return nodes[0]
        return ("cat", nodes, tuple(ops))

    def arguments(self, low, high):
        """`( expr, ... )` when present; the argument nodes."""
        nodes = []
        if self.peek().text == "(":
            self.take()
            if self.peek().text != ")":
                while True:
                    nodes.append(self.expression())
                    if self.peek().text != ",":
                        break
                    self.take()
            self.expect(")")
        if not low <= len(nodes) <= high:
            self.fail()
        return nodes

    def chain(self, statement):
        """A name and its member chain: (kind, node, last). `kind` is "val" or an object kind; `node` is the
        taint node of a value; `last` describes the end of a statement's chain: ("var", Var),
        ("member", name, owner kind) or "call"."""
        token = self.take()
        word = token.low
        case, body = self.case, self.body
        if token.kind == "num":
            return "val", NUM, None
        if token.kind == "hex":                  # &H8000 is -32768
            return "val", NUM, None
        if token.kind == "str":
            return "val", case.literal(token), None
        if token.text == "(":
            node = self.expression()
            self.expect(")")
            return "val", node, None
        if token.kind != "id":
            self.fail(token)
        if word in ("true", "false") or word.startswith("xl") and word in CONSTANTS:
            return "val", NUM, None
        if word in ("empty", "null", "nothing", "vbnullstring"):
            return "val", MAYBE, None
        if word in CONSTANTS:
            return "val", SAFE, None
        if word in BUILTINS:
            low, high, safety = BUILTINS[word]
            args = self.arguments(low, high)
            node = {"num": NUM, "text": SAFE, "never": UNSAFE}.get(safety) or (
                (("join", args) if word == "array" else ("str", args, word)))
            return "val", node, None
        kind = None
        if word == "thisworkbook":
            kind = "wb"
        elif word == "me":
            if case.kinds[body.module] != "document":
                self.fail(token)
            kind = "wb" if body.module == "thisworkbook" else "sheet"
        elif word == "err":
            kind = "err"
        elif word == "createobject":
            self.expect("(")
            literal = self.take()
            if literal.kind != "str" or literal.text[1:-1].lower() not in CLASSES:
                refuse("CreateObject requires one listed literal class", self.text)
            self.expect(")")
            kind = CLASSES[literal.text[1:-1].lower()]
        else:
            var = self.scope_var(token)
            if var is not None:
                if var.kind == "val":
                    if self.peek().text == "(":
                        if not var.array:
                            self.fail()
                        self.arguments(1, 4)
                    return "val", ("var", var.key), ("var", var)
                if var.kind == "pending":
                    self.fail(token)
                kind = var.kind
                if kind == "dict" and self.peek().text == "(":
                    self.arguments(1, 1)
                    return "val", UNSAFE, ("member", "item", "dict")
            else:
                proc = self.find_proc(token)
                if proc is None:
                    self.fail(token)
                return self.call(token, proc, statement)
        return self.members(token, kind, statement)

    def members(self, token, kind, statement):
        last = None
        while self.peek().text == ".":
            self.take()
            name = self.take()
            entry = MEMBERS.get(kind, {}).get(name.low) if name.kind == "id" else None
            if entry is None:
                self.fail(name)
            ret, low, high, flags = entry
            nodes = self.arguments(0, high) if self.peek().text == "(" else []
            if statement and "c" in flags and high and not nodes and self.peek().kind != "end" \
                    and self.peek().text not in ("=", "."):
                while True:                                   # a call statement without parentheses
                    nodes.append(self.expression())
                    if self.peek().text != ",":
                        break
                    self.take()
            if not low <= len(nodes) <= high:
                self.fail(name)
            if ret in OBJECT_KINDS:
                kind = ret
                continue
            if self.peek().text == ".":
                self.fail()
            last = "call" if "c" in flags and self.peek().text != "=" else ("member", name.low, kind)
            return "val", NUM if ret in ("num", "bool") else UNSAFE, last
        if statement:
            self.fail(token)
        return kind, SAFE, None

    def find_proc(self, token):
        word, case = token.low, self.case
        if word in case.names and self.peek().text == ".":
            self.take()
            name = self.take()
            proc = case.procs.get(f"{word}.{name.low}")
            if case.kinds[word] != "standard" or proc is None or not proc.public:
                self.fail(name)
            return proc
        proc = case.procs.get(f"{self.body.module}.{word}")
        if proc is not None:
            return proc
        for other in case.procs.values():
            if other.name == word and other.public and case.kinds[other.module] == "standard":
                return other
        return None

    def call(self, token, proc, statement):
        count = len(proc.params)
        nodes = []
        if self.peek().text == "(":
            nodes = self.arguments(count, count)
        elif statement and count:
            while True:
                nodes.append(self.expression())
                if self.peek().text != ",":
                    break
                self.take()
        if len(nodes) != count:
            self.fail(token)
        for (pname, ptype), node in zip(proc.params, nodes):
            if ptype in OBJECT_TYPES:
                self.fail(token)
            self.case.flows.append((f"{proc.key}.{pname}", node))
        return "val", ("var", f"{proc.key}.{proc.name}") if proc.is_function else UNSAFE, "call"


def check_modules(modules, sheet_count):
    """Check every module of a case. `modules` is [(name, kind, source)]. Returns the lowercase entry points
    ("module.procedure") a call may name and every standard-module procedure name; raises ValueError on the first refused construct."""
    seen = set()
    for name, kind, source in modules:
        low = name.lower()
        if kind == "standard":
            valid = (MODULE_NAME.fullmatch(name) and low not in RESERVED
                     and not SHEET_CODE_NAME.fullmatch(low) and low != "thisworkbook")
        else:
            valid = name == "ThisWorkbook" or (re.fullmatch("Sheet[1-9][0-9]?", name) and 1 <= int(name[5:]) <= sheet_count)
        if not valid or low in seen or kind not in ("standard", "document"):
            raise ValueError(f"invalid or duplicate VBA module name or kind: {escape(str(name))}")
        seen.add(low)
    case = Case(list(modules))
    entry_points = case.run()
    # Every procedure of a standard module, Private ones too: a worksheet formula calls a Private Function (rw17).
    names = {p.key for p in case.procs.values() if case.kinds[p.module] == "standard"}
    return entry_points, names
