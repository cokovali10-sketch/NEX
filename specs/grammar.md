# NEX Grammar (0.1)

This grammar describes the syntax implemented by the NEX 0.1 parser. Indentation
tokens are produced by the lexer.

```ebnf
program        := { enum_decl | struct_decl | const_decl | function } EOF
enum_decl      := "enum" IDENT ":" NEWLINE INDENT variant+ DEDENT
variant        := IDENT NEWLINE
struct_decl    := "struct" IDENT ":" NEWLINE INDENT field+ DEDENT
field          := IDENT ":" type NEWLINE
const_decl     := "const" IDENT [ ":" type ] "=" expression NEWLINE
function       := "fn" IDENT "(" [ params ] ")" [ "->" type ] ":" block
params         := param { "," param } [ "," ]
param          := IDENT ":" type

block          := NEWLINE INDENT statement+ DEDENT
statement      := let_stmt
               | var_stmt
               | if_stmt
               | while_stmt
               | for_stmt
               | match_stmt
               | return_stmt
               | break_stmt
               | continue_stmt
               | assignment
               | field_assignment
               | expr_stmt

let_stmt       := "let" IDENT [ ":" type ] "=" expression NEWLINE
var_stmt       := "var" IDENT [ ":" type ] "=" expression NEWLINE
assignment     := IDENT ( "=" | "+=" | "-=" | "*=" | "/=" | "%=" ) expression NEWLINE
field_assignment := IDENT "." IDENT ( "=" | "+=" | "-=" | "*=" | "/=" | "%=" ) expression NEWLINE
if_stmt        := "if" expression ":" block
                  { "elif" expression ":" block }
                  [ "else" ":" block ]
while_stmt     := "while" expression ":" block
for_stmt       := "for" IDENT "in" expression ".." expression ":" block
match_stmt     := "match" expression ":" NEWLINE INDENT match_arm+ DEDENT
match_arm      := "case" IDENT ":" block
               | "else" ":" block
break_stmt     := "break" NEWLINE
continue_stmt  := "continue" NEWLINE
expr_stmt      := expression NEWLINE

expression     := or_expr
or_expr        := and_expr { ("or" | "||") and_expr }
and_expr       := not_expr { ("and" | "&&") not_expr }
not_expr       := ("not" | "!") not_expr | comparison
comparison     := additive { ("==" | "!=" | "<" | "<=" | ">" | ">=") additive }
additive       := multiplicative { ("+" | "-") multiplicative }
multiplicative := cast { ("*" | "/" | "%") cast }
cast           := unary { "as" type }
unary          := "-" cast | postfix
postfix        := primary { "(" [ arguments ] ")" | "." IDENT | "[" expression "]" }
arguments      := expression { "," expression } [ "," ]
primary        := INT | FLOAT | STRING | CHAR | "true" | "false"
               | IDENT
               | "[" [ expression { "," expression } [ "," ] ] "]"
               | "(" expression ( "," expression )* [ "," ] ")"
               | "(" expression ")"

type           := IDENT
```

Reserved keywords for planned features include `trait`, `impl`, `import`,
`module`, `use`, `extern`, `unsafe`, `async`, and `await`.
