use std::io::{self, Write};

pub fn ask(label: &str) -> io::Result<String> {
	eprint!("{}: ", label);
	io::stderr().flush()?;
	let mut input = String::new();
	io::stdin().read_line(&mut input)?;
	Ok(input.trim().to_string())
}

pub fn confirm(question: &str) -> io::Result<bool> {
	eprint!("{} [Y/n] ", question);
	io::stderr().flush()?;
	let mut input = String::new();
	io::stdin().read_line(&mut input)?;
	let answer = input.trim().to_lowercase();
	Ok(answer.is_empty() || answer == "y" || answer == "yes")
}
