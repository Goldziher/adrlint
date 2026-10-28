pub fn today() -> String {
    jiff::Zoned::now().date().to_string()
}

pub fn slugify(title: &str) -> String {
    let mut slug = String::with_capacity(title.len());
    let mut previous_hyphen = false;
    for character in title.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            previous_hyphen = false;
        } else if !previous_hyphen && !slug.is_empty() {
            slug.push('-');
            previous_hyphen = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() { "adr".to_owned() } else { slug }
}

pub fn render(title: &str, status: &str) -> String {
    format!(
        r#"---
status: {status}
date: {}
decision-makers: []
consulted: []
informed: []
---

# {title}

## Context and Problem Statement

Describe the context and the problem statement here, in two or three sentences.

## Decision Drivers

* The first decision driver
* The second decision driver

## Considered Options

* The first option
* The second option

## Decision Outcome

Chosen option: "The first option", because it best satisfies the decision drivers.

### Consequences

* Good, because the first positive consequence
* Bad, because the first negative consequence

### Confirmation

Describe how compliance with the decision will be confirmed.

## Pros and Cons of the Options

### The first option

* Good, because the first advantage
* Bad, because the first disadvantage

### The second option

* Good, because the first advantage
* Bad, because the first disadvantage

## More Information

* [MADR](https://adr.github.io/madr/)
"#,
        today()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_slugify_a_title() {
        assert_eq!(slugify("Use Rust for the CLI"), "use-rust-for-the-cli");
        assert_eq!(slugify("  ADR: MADR 4.0!  "), "adr-madr-4-0");
        assert_eq!(slugify("!!!"), "adr");
    }

    #[test]
    fn should_render_the_title_and_status() {
        let document = render("Choose a database", "proposed");
        assert!(document.starts_with("---\nstatus: proposed\n"));
        assert!(document.contains("# Choose a database\n"));
        assert!(document.contains("## Decision Outcome\n"));
    }
}
