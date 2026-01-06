use bumpalo::Bump;
use php_rs::parser::lexer::Lexer;
use php_rs::parser::parser::Parser;

#[test]
fn test_short_array_unexpected_semicolon() {
    let code = "<?php var_dump([1;]);";
    let lexer = Lexer::new(code.as_bytes());
    let bump = Bump::new();
    let mut parser = Parser::new(lexer, &bump);
    let program = parser.parse_program();

    assert!(
        program.errors.iter().any(|error| error.message.contains("Unexpected ';'")),
        "expected an unexpected-semicolon parse error, got: {:?}",
        program.errors
    );
}
