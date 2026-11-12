# NEX Grammar (0.1)

This grammar describes the syntax implemented by the NEX 0.1 parser. Indentation
tokens are produced by the lexer.

```ebnf
program        := { const_decl | function } EOF
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
               | return_stmt
               | break_stmt
               | continue_stmt
               | assignment
               | expr_stmt

let_stmt       := "let" IDENT [ ":" type ] "=" expression NEWLINE
var_stmt       := "var" IDENT [ ":" type ] "=" expression NEWLINE
assignment     := IDENT "=" expression NEWLINE
if_stmt        := "if" expression ":" block
                  { "elif" expression ":" block }
                  [ "else" ":" block ]
while_stmt     := "while" expression ":" block
for_stmt       := "for" IDENT "in" expression ".." expression ":" block
return_stmt    := "return" [ expression ] NEWLINE
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
postfix        := primary { "(" [ arguments ] ")" }
arguments      := expression { "," expression } [ "," ]
primary        := INT | FLOAT | STRING | CHAR | "true" | "false"
               | IDENT
               | "(" expression ")"

type           := IDENT
```

Reserved keywords for planned features include `struct`, `enum`, `trait`,
`impl`, `import`, `module`, `use`, `const`, `extern`, `unsafe`, `async`,
`await`, `match`, `for`, and `in`.
