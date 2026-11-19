import pathlib
p = pathlib.Path('compiler/parser/src/lib.rs')
s = p.read_text(encoding='utf-8')
s = s.replace('                        tuple: None,\n                        span: receiver_span,', '                        tuple: None,\n                        generic_args: Vec::new(),\n                        span: receiver_span,')
s = s.replace('                    tuple: Some(elems),\n                    span: start.merge(end_span),', '                    tuple: Some(elems),\n                    generic_args: Vec::new(),\n                    span: start.merge(end_span),')
s = s.replace('                    tuple: first.tuple,\n                    span: first.span.merge(end_span),', '                    tuple: first.tuple,\n                    generic_args: first.generic_args.clone(),\n                    span: first.span.merge(end_span),')
s = s.replace('                    tuple: None,\n                    span: start,\n                };', '                    tuple: None,\n                    generic_args: Vec::new(),\n                    span: start,\n                };')
s = s.replace('                    tuple: None,\n                    span: start.merge(end_span),', '                    tuple: None,\n                    generic_args: Vec::new(),\n                    span: start.merge(end_span),')
s = s.replace('                tuple: None,\n                span,\n            }', '                tuple: None,\n                generic_args: Vec::new(),\n                span,\n            }')
s = s.replace('                name: "<error>".to_string(),\n                array: None,\n                tuple: None,\n                span,\n            }', '                name: "<error>".to_string(),\n                array: None,\n                tuple: None,\n                generic_args: Vec::new(),\n                span,\n            }')
p.write_text(s, encoding='utf-8')
print('done')
PY
