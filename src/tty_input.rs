use crate::traits::GetInput;

struct TtyInput;

impl GetInput for TtyInput {
    fn record(&mut self) -> String {
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        input.trim().to_string()
    }
}
