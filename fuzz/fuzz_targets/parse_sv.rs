use honggfuzz::fuzz;
use veryl_parser::{veryl_parser::parse, veryl_grammar::VerylGrammar};

fn main() {
    let mut grammar = VerylGrammar::new();
    loop {
        fuzz!(|data: &[u8]| {
            let source: &str = std::str::from_utf8(data).unwrap();
            let file_name = "fuzz.sv".to_string(); // sv file extension is required
            let _ = parse(source, file_name, &mut grammar);
        });
    }
}