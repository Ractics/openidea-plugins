//! "Add validation constraints" on a Doctrine entity: `#[Assert\…]` attributes derived from each
//! field's `#[ORM\Column]` (nullability, default, length, unique, type, e-mail name).
use std::collections::BTreeMap;

use crate::entities::{self, argument};
use crate::intentions::{self, Action};
use crate::php_text::{self, AttributeUse, Target};

const CONSTRAINTS: &str = "Symfony\\Component\\Validator\\Constraints";
const UNIQUE_ENTITY: &str = "Symfony\\Bridge\\Doctrine\\Validator\\Constraints\\UniqueEntity";
const COLUMN: &str = "Doctrine\\ORM\\Mapping\\Column";
/// Doctrine date and time types, and the PHP classes of untyped columns.
const DATE_TYPES: [&str; 10] = [
    "date", "date_immutable", "datetime", "datetime_immutable", "datetimetz", "datetimetz_immutable", "time", "time_immutable",
    "datetimeinterface", "datetimeimmutable",
];

/// The constraints one field lacks, and where they go.
struct Missing {
    property: String,
    /// Line start of the property declaration and its indentation.
    at: usize,
    indent: String,
    /// `NotBlank`, `Length(max: 255)`… without the alias.
    constraints: Vec<String>,
    unique: bool,
    /// Caret range that offers the per-property action.
    span: std::ops::Range<usize>,
}

fn short(name: &str) -> &str {
    php_text::short_name(name)
}

fn on_property<'a>(attributes: &'a [AttributeUse], property: &'a str) -> impl Iterator<Item = &'a AttributeUse> + 'a {
    attributes.iter().filter(move |attribute| matches!(&attribute.target, Target::Property { name } if name == property))
}

/// Constraints derived from a column: presence, type, length, e-mail.
fn derived(type_name: &str, name: &str, required: bool, length: Option<&str>) -> Vec<String> {
    let kind = type_name.to_ascii_lowercase();
    let text = matches!(kind.as_str(), "string" | "text");
    let mut found = Vec::new();
    if required {
        found.push(if text { "NotBlank".into() } else { "NotNull".into() });
    }
    match kind.as_str() {
        "integer" | "smallint" | "bigint" | "int" => found.push("Type('integer')".into()),
        "decimal" => found.push("Type('numeric')".into()),
        "boolean" | "bool" => found.push("Type('bool')".into()),
        kind if DATE_TYPES.contains(&kind) => found.push("Type(\\DateTimeInterface::class)".into()),
        _ => {}
    }
    if let Some(length) = length.filter(|length| !length.is_empty() && length.bytes().all(|byte| byte.is_ascii_digit())) {
        found.push(format!("Length(max: {length})"));
    }
    let column = entities::snake_case(name);
    if column == "email" || column.ends_with("_email") {
        found.push("Email".into());
    }
    found
}

/// A property default other than `null` (`private string $status = 'draft';`).
fn has_default(masked: &str, name_end: usize) -> bool {
    let at = php_text::skip_whitespace(masked, name_end);
    let Some(value) = masked[at..].strip_prefix('=') else { return false };
    let value = value[..value.find([';', ',']).unwrap_or(value.len())].trim();
    !value.eq_ignore_ascii_case("null")
}

struct Entity {
    short_name: String,
    fields: Vec<Missing>,
    /// Caret range of the class attributes and `class` line.
    header: std::ops::Range<usize>,
    class_line: usize,
}

fn missing(text: &str) -> Option<Entity> {
    let entity = entities::parse("", text)?;
    let model = php_text::class_model(text)?;
    let masked = php_text::mask(text);
    let attributes = php_text::attributes(text);
    let unique_declared = |field: &str| {
        attributes.iter().any(|attribute| {
            matches!(attribute.target, Target::Class { .. })
                && short(&attribute.name) == "UniqueEntity"
                && [format!("'{field}'"), format!("\"{field}\"")].iter().any(|quoted| text[attribute.arguments.clone()].contains(quoted.as_str()))
        })
    };

    let mut fields = Vec::new();
    for field in entity.fields.iter().filter(|field| !field.id) {
        let property = model.properties.iter().find(|property| property.name == field.name)?;
        let mine: Vec<&AttributeUse> = on_property(&attributes, &field.name).collect();
        let arguments = mine.iter().find(|attribute| attribute.name == COLUMN).map(|column| &text[column.arguments.clone()]).unwrap_or_default();
        let defaulted = argument(arguments, "options").is_some_and(|options| options.contains("'default'") || options.contains("\"default\""))
            || has_default(&masked, property.name_range.end);
        let length = argument(arguments, "length");
        let existing: Vec<&str> = mine.iter().map(|attribute| short(&attribute.name)).collect();
        let constraints: Vec<String> = derived(&field.type_name, &field.name, !field.nullable && !defaulted, length)
            .into_iter()
            .filter(|constraint| !existing.contains(&constraint.split('(').next().unwrap_or(constraint)))
            .collect();
        let unique = argument(arguments, "unique") == Some("true") && !unique_declared(&field.name);
        if constraints.is_empty() && !unique {
            continue;
        }
        let first = mine.iter().filter_map(|attribute| text[..attribute.range.start].rfind("#[")).min().unwrap_or(property.name_range.start);
        let at = intentions::line_start(text, property.name_range.start);
        fields.push(Missing {
            property: field.name.clone(),
            at,
            indent: intentions::indent_at(text, property.name_range.start).to_owned(),
            constraints,
            unique,
            span: intentions::line_start(text, first.min(at))..php_text::line_end(&masked, property.name_range.end),
        });
    }

    let first_class_attribute = attributes
        .iter()
        .filter(|attribute| matches!(attribute.target, Target::Class { .. }))
        .filter_map(|attribute| text[..attribute.range.start].rfind("#["))
        .min()
        .unwrap_or(model.name_range.start);
    Some(Entity {
        short_name: php_text::short_name(&model.fqcn).to_owned(),
        fields,
        header: intentions::line_start(text, first_class_attribute)..php_text::line_end(&masked, model.name_range.end),
        class_line: model.name_range.start,
    })
}

/// Builds the edits that add `fields`' constraints, with the needed imports.
fn action(text: &str, title: String, fields: &[&Missing], class_line: usize) -> Option<Action> {
    let eol = intentions::eol_of(text);
    let mut inserts: BTreeMap<usize, String> = BTreeMap::new();
    let mut add = |at: usize, insert: String| {
        let entry = inserts.entry(at).or_default();
        let insert = if !entry.is_empty() && insert.starts_with(eol) { insert[eol.len()..].to_owned() } else { insert };
        entry.push_str(&insert);
    };

    if fields.iter().any(|field| !field.constraints.is_empty()) {
        let alias = match php_text::imports(text).into_iter().find(|import| import.fqcn == CONSTRAINTS) {
            Some(import) => import.alias,
            None => {
                if php_text::imports(text).iter().any(|import| import.alias.eq_ignore_ascii_case("Assert")) {
                    return None;
                }
                let edit = php_text::insert_use(text, CONSTRAINTS)?;
                add(edit.offset, edit.text.replace("Constraints;", "Constraints as Assert;"));
                "Assert".to_owned()
            }
        };
        for field in fields {
            let lines: String = field.constraints.iter().map(|constraint| format!("{}#[{alias}\\{constraint}]{eol}", field.indent)).collect();
            add(field.at, lines);
        }
    }

    if fields.iter().any(|field| field.unique) {
        let (import, unique) = intentions::import_for(text, UNIQUE_ENTITY)?;
        if let Some((range, line)) = import {
            add(range.start, line);
        }
        let indent = intentions::indent_at(text, class_line).to_owned();
        let at = intentions::line_start(text, class_line);
        let lines: String = fields.iter().filter(|field| field.unique).map(|field| format!("{indent}#[{unique}(fields: ['{}'])]{eol}", field.property)).collect();
        add(at, lines);
    }

    Some(Action { title, kind: "refactor.rewrite", edits: inserts.into_iter().map(|(at, insert)| (at..at, insert)).collect() })
}

/// Per-property and whole-entity actions for the caret at `at` in an entity file.
pub(crate) fn actions(text: &str, at: usize) -> Vec<Action> {
    let Some(Entity { short_name, fields, header, class_line }) = missing(text) else { return Vec::new() };
    if fields.is_empty() {
        return Vec::new();
    }
    let on_header = header.contains(&at) || header.end == at;
    let here = fields.iter().find(|field| field.span.contains(&at) || field.span.end == at);

    let mut found = Vec::new();
    if let Some(field) = here {
        found.extend(action(text, format!("Add validation constraints to ${}", field.property), &[field], class_line));
    }
    if on_header || here.is_some_and(|_| fields.len() > 1) {
        let all: Vec<&Missing> = fields.iter().collect();
        found.extend(action(text, format!("Add constraints to all fields of {short_name}"), &all, class_line));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const POST: &str = "<?php\nnamespace App\\Entity;\n\nuse Doctrine\\DBAL\\Types\\Types;\nuse Doctrine\\ORM\\Mapping as ORM;\n\n#[ORM\\Entity]\nclass Post\n{\n    #[ORM\\Id, ORM\\GeneratedValue, ORM\\Column]\n    private ?int $id = null;\n\n    #[ORM\\Column(length: 255)]\n    private ?string $title = null;\n\n    #[ORM\\Column(length: 180, unique: true)]\n    private ?string $contactEmail = null;\n\n    #[ORM\\Column(type: Types::DECIMAL, precision: 10, scale: 2, nullable: true)]\n    private ?string $price = null;\n\n    #[ORM\\Column]\n    private bool $published = false;\n\n    #[ORM\\Column(type: Types::DATETIME_IMMUTABLE)]\n    private ?\\DateTimeImmutable $createdAt = null;\n\n    #[ORM\\Column(type: 'integer', options: ['default' => 0])]\n    private int $views;\n}\n";

    fn apply(text: &str, action: &Action) -> String {
        let mut out = text.to_owned();
        for (range, insert) in action.edits.iter().rev() {
            out.replace_range(range.clone(), insert);
        }
        out
    }

    fn at(text: &str, needle: &str) -> usize {
        text.find(needle).unwrap()
    }

    #[test]
    fn a_property_gets_constraints_from_its_column_and_the_assert_import() {
        let found = actions(POST, at(POST, "$title"));
        assert_eq!(found.iter().map(|action| action.title.as_str()).collect::<Vec<_>>(), ["Add validation constraints to $title", "Add constraints to all fields of Post"]);
        let out = apply(POST, &found[0]);
        assert!(out.contains("use Doctrine\\ORM\\Mapping as ORM;\nuse Symfony\\Component\\Validator\\Constraints as Assert;\n"));
        assert!(out.contains("    #[ORM\\Column(length: 255)]\n    #[Assert\\NotBlank]\n    #[Assert\\Length(max: 255)]\n    private ?string $title = null;"));
        assert!(!out.contains("UniqueEntity"));
    }

    #[test]
    fn all_fields_cover_types_email_unique_and_defaults() {
        let found = actions(POST, at(POST, "class Post"));
        assert_eq!(found.len(), 1);
        let out = apply(POST, &found[0]);
        assert!(out.contains("use Symfony\\Component\\Validator\\Constraints as Assert;\nuse Symfony\\Bridge\\Doctrine\\Validator\\Constraints\\UniqueEntity;\n"));
        assert!(out.contains("#[ORM\\Entity]\n#[UniqueEntity(fields: ['contactEmail'])]\nclass Post"));
        assert!(out.contains("    #[Assert\\NotBlank]\n    #[Assert\\Length(max: 180)]\n    #[Assert\\Email]\n    private ?string $contactEmail"));
        assert!(out.contains("    #[Assert\\Type('numeric')]\n    private ?string $price"));
        assert!(out.contains("    #[ORM\\Column]\n    #[Assert\\Type('bool')]\n    private bool $published"));
        assert!(out.contains("    #[Assert\\NotNull]\n    #[Assert\\Type(\\DateTimeInterface::class)]\n    private ?\\DateTimeImmutable $createdAt"));
        assert!(out.contains("    #[ORM\\Column(type: 'integer', options: ['default' => 0])]\n    #[Assert\\Type('integer')]\n    private int $views;"));
        assert!(out.contains("ORM\\Column]\n    private ?int $id"));
        assert!(actions(&out, at(&out, "class Post")).is_empty(), "applying twice adds nothing");
    }

    #[test]
    fn existing_constraints_and_aliases_are_kept() {
        let text = POST
            .replace("use Doctrine\\ORM\\Mapping as ORM;\n", "use Doctrine\\ORM\\Mapping as ORM;\nuse Symfony\\Component\\Validator\\Constraints as Check;\nuse Symfony\\Bridge\\Doctrine\\Validator\\Constraints\\UniqueEntity;\n")
            .replace("#[ORM\\Entity]\n", "#[ORM\\Entity]\n#[UniqueEntity(fields: ['contactEmail'])]\n")
            .replace("    #[ORM\\Column(length: 255)]\n", "    #[ORM\\Column(length: 255)]\n    #[Check\\NotBlank]\n");
        let found = actions(&text, at(&text, "$title"));
        let out = apply(&text, &found[0]);
        assert!(out.contains("    #[Check\\NotBlank]\n    #[Check\\Length(max: 255)]\n    private ?string $title"));
        assert_eq!(out.matches("NotBlank").count(), 1);
        assert_eq!(out.matches("use Symfony").count(), 2);
        let all = apply(&text, &actions(&text, at(&text, "class Post"))[0]);
        assert_eq!(all.matches("UniqueEntity(").count(), 1);
    }

    #[test]
    fn not_offered_outside_entities_or_away_from_fields() {
        assert!(actions("<?php\nclass Post\n{\n    private string $title;\n}\n", 20).is_empty());
        assert!(actions(POST, at(POST, "private ?int $id")).is_empty());
        let clash = POST.replace("use Doctrine\\ORM\\Mapping as ORM;\n", "use Doctrine\\ORM\\Mapping as ORM;\nuse App\\Assert;\n");
        assert!(actions(&clash, at(&clash, "$title")).is_empty());
    }
}
