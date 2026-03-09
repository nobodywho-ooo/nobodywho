pub const PYTHON_GRAMMAR: &str = r##"
r##"
# Simplified Python Grammar — Permissive / Over-accepting

ws  ::= [ \t]*
wsp ::= [ \t]+
nl  ::= "\x0A"
nls ::= (ws nl)*
lineend ::= ws ("#" [^\x0A]*)? nl

NAME   ::= [a-zA-Z_] [a-zA-Z0-9_]*
NUMBER ::= [0-9] [0-9a-fA-FxXoObBjJ_.]*
STRING ::= [rRuUbBfF]? [rRbBfF]? ("\"\"\"" ([^"\\] | "\\" [^\x0A] | "\"" [^"\\] | "\"\"" [^"\\])* "\"\"\"" | "'''" ([^'\\] | "\\" [^\x0A] | "'" [^'\\] | "''" [^'\\])* "'''" | "\"" ([^"\\\x0A] | "\\" [^\x0A])* "\"" | "'" ([^'\\\x0A] | "\\" [^\x0A])* "'")

atom ::= STRING (ws STRING)* | NUMBER | NAME | "True" | "False" | "None" | "..." | "(" ws exprlist? ws ")" | "[" ws exprlist? ws "]" | "{" ws dictitems? ws "}"
dictitems ::= dictitem (ws "," ws dictitem)* (ws ",")?
dictitem  ::= "**" ws expr | expr (ws ":" ws expr)?

expr ::= prefix* atom postfix* (ws binop ws prefix* atom postfix*)*
prefix  ::= ("not" | "await" | "lambda" wsp (NAME (ws "," ws NAME)* (ws ",")?)? ":") wsp | [+~*-] ws
postfix ::= "(" ws args? ws ")" | "[" ws slicelist ws "]" | "." ws NAME | wsp "if" wsp expr wsp "else" wsp expr | wsp "for" wsp exprlist wsp "in" wsp expr (wsp "if" wsp expr)*
binop   ::= "**" | "//" | "<<" | ">>" | "<=" | ">=" | "==" | "!=" | ":=" | "not" wsp "in" | "is" wsp "not" | "is" | "in" | "or" | "and" | [+*/@%&|^<>-]

slicelist ::= slice (ws "," ws slice)* (ws ",")?
slice     ::= expr? ws ":" ws expr? (ws ":" ws expr?)? | expr

args ::= arg (ws "," ws arg)* (ws ",")?
arg  ::= "**" ws expr | "*" ws expr | NAME ws "=" ws expr | expr

exprlist ::= expritem (ws "," ws expritem)* (ws ",")?
expritem ::= "*" ws expr | expr

root ::= nls (line nls)* ws
line ::= ws compound ":" ws simplestmts lineend | ws compound ":" lineend | ws simplestmts lineend

simplestmts ::= simplestmt (ws ";" ws simplestmt)*
simplestmt  ::= fromstmt | keyword wsp exprlist | keyword | targets ws assignop ws exprlist | targets (ws "=" ws exprlist)+ | targets ws ":" ws expr (ws "=" ws exprlist)? | exprlist
keyword     ::= "return" | "raise" | "yield" wsp "from" | "yield" | "assert" | "del" | "global" | "nonlocal" | "import" | "pass" | "break" | "continue"
fromstmt    ::= "from" wsp NAME ("." NAME)* wsp "import" wsp importtargets
assignop    ::= "+=" | "-=" | "*=" | "@=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" | "**=" | "//="
targets     ::= expritem (ws "," ws expritem)* (ws ",")?

compound ::= decorators? "def" wsp NAME ws "(" ws params? ws ")" (ws "->" ws expr)? | decorators? "async" wsp "def" wsp NAME ws "(" ws params? ws ")" (ws "->" ws expr)? | decorators? "class" wsp NAME (ws "(" ws args? ws ")")? | "if" wsp expr | "elif" wsp expr | "else" | "while" wsp expr | "for" wsp targets wsp "in" wsp exprlist | "async" wsp "for" wsp targets wsp "in" wsp exprlist | "with" wsp expr (ws "," ws expr)* | "async" wsp "with" wsp expr (ws "," ws expr)* | "try" | "except" (wsp expr (wsp "as" wsp NAME)?)? | "finally" | "match" wsp expr | "case" wsp expr (wsp "if" wsp expr)?
decorators  ::= (ws "@" ws expr lineend nls ws)*
params      ::= param (ws "," ws param)* (ws ",")?
param       ::= "**" ws NAME (ws ":" ws expr)? | "*" ws NAME? (ws ":" ws expr)? | NAME (ws ":" ws expr)? (ws "=" ws expr)? | "/"
importtargets ::= "(" ws NAME (wsp "as" wsp NAME)? (ws "," ws NAME (wsp "as" wsp NAME)?)* (ws ",")? ws ")" | NAME (wsp "as" wsp NAME)? (ws "," ws NAME (wsp "as" wsp NAME)?)* | "*"
"##;
