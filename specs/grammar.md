# NEX Grammar (1.0)

This grammar summarizes the syntax accepted by the NEX 1.0 compiler.
Indentation tokens (`INDENT`/`DEDENT`) are produced by the lexer; four spaces
are one level. Top-level items may be marked `pub`.

```ebnf
program        := { item } EOF

item           := module_decl | use_decl | import_decl | enum_decl
                | struct_decl | trait_decl | impl_block | type_alias
                | const_decl | extern_fn | function

module_decl    := "module" IDENT ":" block
use_decl       := "use" IDENT { "." IDENT } [ "from" STRING ] NEWLINE
import_decl    := "import" STRING NEWLINE
                | "from" STRING "import" IDENT NEWLINE

enum_decl      := [ "pub" ] "enum" IDENT ":" NEWLINE INDENT variant+ DEDENT
variant        := IDENT NEWLINE

struct_decl    := [ "pub" ] "struct" IDENT [ "<" generic_params ">" ] ":"
                  NEWLINE INDENT field+ DEDENT
field          := IDENT ":" type NEWLINE
generic_params := IDENT { "," IDENT }

trait_decl     := [ "pub" ] "trait" IDENT ":" NEWLINE INDENT method_sig+ DEDENT
method_sig     := "fn" IDENT "(" [ params ] ")" [ "->" type ] NEWLINE

impl_block     := "impl" [ IDENT "for" ] IDENT ":"
                  NEWLINE INDENT function+ DEDENT

type_alias     := [ "pub" ] "type" IDENT "=" type NEWLINE
const_decl     := [ "pub" ] "const" IDENT [ ":" type ] "=" expression NEWLINE
extern_fn      := "extern" STRING "fn" IDENT "(" [ params ] ")" [ "->" type ] NEWLINE
function       := [ "pub" ] "fn" IDENT [ "<" generic_params ">" ] "("
                  [ params ] ")" [ "->" type ] ":" block

params         := param { "," param } [ "," ]
param          := "self" | IDENT ":" type

block          := NEWLINE INDENT statement+ DEDENT
statement      := let_stmt
                | var_stmt
                | if_stmt
                | while_stmt
                | for_stmt
                | match_stmt
                | unsafe_stmt
                | return_stmt
                | break_stmt
                | continue_stmt
                | parallel_assignment
                | assignment
                | field_assignment
                | index_assignment
                | expr_stmt

let_stmt       := ( "let" | "var" ) ( IDENT | "(" IDENT { "," IDENT } ")" )
                  [ ":" type ] "=" expression NEWLINE
var_stmt       := let_stmt

if_stmt        := "if" expression ":" block
                  { "elif" expression ":" block }
                  [ "else" ":" block ]
while_stmt     := "while" expression ":" block
for_stmt       := "for" IDENT "in" expression [ ".." expression ] ":" block

match_stmt     := "match" expression ":" NEWLINE INDENT match_arm+ DEDENT
match_arm      := "case" pattern [ "if" expression ] ":" block
                | "else" ":" block
pattern        := literal [ ".." literal ] | IDENT

unsafe_stmt    := "unsafe" ":" block
return_stmt    := "return" [ expression ] NEWLINE
break_stmt     := "break" NEWLINE
continue_stmt  := "continue" NEWLINE

parallel_assignment := IDENT { "," IDENT } "=" expression { "," expression } NEWLINE
assignment     := IDENT ( "=" | "+=" | "-=" | "*=" | "/=" | "%=" ) expression NEWLINE
field_assignment := postfix "." IDENT
                  ( "=" | "+=" | "-=" | "*=" | "/=" | "%=" ) expression NEWLINE
index_assignment := postfix "[" expression "]" "=" expression NEWLINE
expr_stmt      := expression NEWLINE

expression     := or_expr
or_expr        := and_expr { ( "or" | "||" ) and_expr }
and_expr       := bit_or { ( "and" | "&&" ) bit_or }
bit_or         := bit_xor { "|" bit_xor }
bit_xor        := bit_and { "^" bit_and }
bit_and        := shift { "&" shift }
shift          := comparison { ( "<<" | ">>" ) comparison }
comparison     := additive { ( "==" | "!=" | "<" | "<=" | ">" | ">=" ) additive }
additive       := multiplicative { ( "+" | "-" ) multiplicative }
multiplicative := cast { ( "*" | "/" | "%" ) cast }
cast           := unary { "as" type }
unary          := ( "-" | "not" | "!" | "~" ) unary | postfix
postfix        := primary { "(" [ arguments ] ")" | "." IDENT | "[" slice "]" }
slice          := [ expression ] [ ".." [ expression ] ]
arguments      := expression { "," expression } [ "," ]

primary        := INT | FLOAT | STRING | "f" STRING | CHAR
                | "true" | "false" | IDENT
                | "[" [ expression { "," expression } [ "," ] ] "]"
                | "(" expression ( "," expression )* [ "," ] ")"
                | "(" expression ")"
                | IDENT "(" [ arguments ] ")"

type           := IDENT
literal        := INT | FLOAT | STRING | CHAR | "true" | "false"
```

Notes:

- `for x in collection` and `for i in start..end` share the same statement;
  the first form is lowered to an indexed loop plus an element binding.
- Generic type arguments in type positions (`Box<i64>`) are not accepted yet.
  Generic calls and constructors infer their type arguments, e.g. `Box(42)`.
- `pattern` covers literals, inclusive ranges, booleans, characters, strings,
  enum variants (identifiers), and guards. Scalar `match` statements require an
  `else` arm.
