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

pub fn confirm_default_no(question: &str) -> io::Result<bool> {
	eprint!("{} [y/N] ", question);
	io::stderr().flush()?;
	let mut input = String::new();
	io::stdin().read_line(&mut input)?;
	let answer = input.trim().to_lowercase();
	Ok(answer == "y" || answer == "yes")
}

pub fn ask_with_default(label: &str, default: &str) -> io::Result<String> {
	eprint!("{} [{}]: ", label, default);
	io::stderr().flush()?;
	let mut input = String::new();
	io::stdin().read_line(&mut input)?;
	let answer = input.trim();
	Ok(if answer.is_empty() { default.to_string() } else { answer.to_string() })
}

pub fn is_interactive() -> bool {
	unsafe { libc::isatty(libc::STDIN_FILENO) == 1 }
}
