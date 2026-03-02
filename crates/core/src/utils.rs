use regex::Regex;

/// Unpack Dean Edwards packed JavaScript
pub fn unpack_de(packed: &str, base: u32, count: usize, symtab: Vec<&str>) -> String {
    let mut result = packed.to_string();

    // Replace tokens from highest index down
    for i in (0..count).rev() {
        if let Some(word) = symtab.get(i) {
            if word.is_empty() {
                continue;
            }

            let token = format!(r"\b{}\b", to_base(i, base));
            let re = Regex::new(&token).unwrap();
            result = re.replace_all(&result, *word).to_string();
        }
    }

    result
}

/// Convert number to base-N string
fn to_base(mut num: usize, base: u32) -> String {
    if num == 0 {
        return "0".to_string();
    }

    let mut out = String::new();
    while num > 0 {
        let rem = num % base as usize;
        let ch = if rem > 35 {
            std::char::from_u32((rem as u32) + 29).unwrap()
        } else {
            std::char::from_digit(rem as u32, 36).unwrap()
        };
        out.insert(0, ch);
        num /= base as usize;
    }
    out
}
