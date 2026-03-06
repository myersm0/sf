use std::io::{self, Read, Write};

pub struct PickerItem {
	pub key: String,
	pub display: String,
}

pub fn run_picker(items: &[PickerItem], title: &str) -> Option<String> {
	if items.is_empty() {
		eprintln!("no results");
		return None;
	}

	let mut stderr = io::stderr();
	let count = items.len();
	let number_width = if count >= 100 { 3 } else if count >= 10 { 2 } else { 1 };

	if !title.is_empty() {
		writeln!(stderr, " {}", title).ok();
	}

	for (index, item) in items.iter().enumerate() {
		writeln!(
			stderr,
			" {:>width$}) {}: {}",
			index + 1,
			item.key,
			item.display,
			width = number_width,
		).ok();
	}
	writeln!(stderr).ok();
	write!(stderr, " go to (q to cancel): ").ok();
	stderr.flush().ok();

	let tty = match std::fs::File::open("/dev/tty") {
		Ok(f) => f,
		Err(_) => return None,
	};

	#[cfg(unix)]
	{
		use std::os::unix::io::AsRawFd;
		let fd = tty.as_raw_fd();
		let orig = match set_raw_mode(fd) {
			Some(t) => t,
			None => return None,
		};

		let result = read_selection(&tty, count, &mut stderr);
		restore_mode(fd, &orig);
		result.map(|index| items[index].key.clone())
	}

	#[cfg(not(unix))]
	{
		let mut input = String::new();
		io::stdin().read_line(&mut input).ok();
		let trimmed = input.trim();
		if trimmed == "q" || trimmed == "Q" {
			return None;
		}
		trimmed.parse::<usize>().ok()
			.filter(|&n| n >= 1 && n <= count)
			.map(|n| items[n - 1].key.clone())
	}
}

#[cfg(unix)]
fn read_selection(tty: &std::fs::File, count: usize, stderr: &mut io::Stderr) -> Option<usize> {
	let mut buf = String::new();
	let mut reader = io::BufReader::new(tty);

	loop {
		let mut byte = [0u8; 1];
		if reader.read(&mut byte).unwrap_or(0) == 0 {
			return None;
		}
		let ch = byte[0] as char;

		match ch {
			'q' | 'Q' | '\x1b' => {
				writeln!(stderr).ok();
				return None;
			}
			'\r' | '\n' => {
				writeln!(stderr).ok();
				if buf.is_empty() {
					return None;
				}
				return buf.parse::<usize>().ok()
					.filter(|&n| n >= 1 && n <= count)
					.map(|n| n - 1);
			}
			'0'..='9' => {
				let mut candidate = buf.clone();
				candidate.push(ch);
				let n = candidate.parse::<usize>().unwrap_or(0);

				if n < 1 || n > count {
					continue;
				}

				buf = candidate;
				write!(stderr, "{}", ch).ok();
				stderr.flush().ok();

				if n * 10 > count {
					writeln!(stderr).ok();
					return Some(n - 1);
				}
			}
			'\x7f' | '\x08' => {
				if buf.pop().is_some() {
					write!(stderr, "\x08 \x08").ok();
					stderr.flush().ok();
				}
			}
			_ => {}
		}
	}
}

#[cfg(unix)]
fn set_raw_mode(fd: i32) -> Option<libc::termios> {
	unsafe {
		let mut orig: libc::termios = std::mem::zeroed();
		if libc::tcgetattr(fd, &mut orig) != 0 {
			return None;
		}
		let mut raw = orig;
		raw.c_lflag &= !(libc::ICANON | libc::ECHO);
		raw.c_cc[libc::VMIN] = 1;
		raw.c_cc[libc::VTIME] = 0;
		if libc::tcsetattr(fd, libc::TCSANOW, &raw) != 0 {
			return None;
		}
		Some(orig)
	}
}

#[cfg(unix)]
fn restore_mode(fd: i32, orig: &libc::termios) {
	unsafe {
		libc::tcsetattr(fd, libc::TCSANOW, orig);
	}
}
