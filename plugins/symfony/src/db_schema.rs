//! Tables, columns, keys and associations from Doctrine attribute mappings, with Doctrine's naming
//! strategy for every name the mapping leaves out; the database comparison and the schema diagram.
//! Source only: nothing runs. Annotation, XML and YAML mappings are listed as unreadable.
use crate::entities::{self, argument, string_value};
use crate::php_text::{self, AttributeUse, Target, short_name};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const MAPPING: &str = "Doctrine\\ORM\\Mapping\\";
/// Tables Doctrine tooling creates itself.
const INTERNAL: [&str; 5] = ["doctrine_migration_versions", "migration_versions", "messenger_messages", "sqlite_sequence", "spatial_ref_sys"];
pub(crate) const MAX_LIVE_TABLES: usize = 512;

/// Doctrine's naming strategies; the Symfony recipe configures `NumberAware`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Naming {
    Default,
    Underscore,
    NumberAware,
}

impl Naming {
    /// From `config/packages/doctrine.yaml` (`None` when missing: the recipe default).
    pub(crate) fn from_config(doctrine_yaml: Option<&str>) -> Self {
        match doctrine_yaml {
            None => Self::NumberAware,
            Some(text) if text.contains("naming_strategy.underscore_number_aware") => Self::NumberAware,
            Some(text) if text.contains("naming_strategy.underscore") => Self::Underscore,
            // DoctrineBundle's own default when the key is absent.
            Some(_) => Self::Default,
        }
    }

    fn underscore(self, name: &str) -> String {
        let mut out = String::new();
        let mut previous: Option<char> = None;
        for character in name.chars() {
            let after = previous.is_some_and(|before| before.is_ascii_lowercase() || (self == Self::NumberAware && before.is_ascii_digit()));
            if character.is_ascii_uppercase() && after {
                out.push('_');
            }
            out.extend(character.to_lowercase());
            previous = Some(character);
        }
        out
    }

    pub(crate) fn table(self, class: &str) -> String {
        match self {
            Self::Default => short_name(class).to_owned(),
            _ => self.underscore(short_name(class)),
        }
    }

    fn column(self, property: &str) -> String {
        match self {
            Self::Default => property.to_owned(),
            _ => self.underscore(property),
        }
    }

    fn join_column(self, property: &str) -> String {
        format!("{}_id", self.column(property))
    }

    fn join_table(self, source: &str, target: &str) -> String {
        let name = format!("{}_{}", self.table(source), self.table(target));
        if self == Self::Default { name.to_lowercase() } else { name }
    }

    fn join_key(self, class: &str) -> String {
        let name = format!("{}_id", self.table(class));
        if self == Self::Default { name.to_lowercase() } else { name }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Place {
    pub path: String,
    /// 0-based.
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Column {
    pub name: String,
    /// `string(255), nullable`, `ManyToOne → User`.
    pub kind: String,
    pub open: Option<Place>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Index {
    pub columns: Vec<String>,
    /// `primary`, `unique` or `index`.
    pub kind: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Foreign {
    pub column: String,
    /// The referenced table; `None` when the target class is not a mapped entity.
    pub on: Option<String>,
    pub references: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Table {
    pub name: String,
    /// The mapped entity; `None` for a many-to-many join table.
    pub entity: Option<String>,
    pub columns: Vec<Column>,
    pub indexes: Vec<Index>,
    pub foreigns: Vec<Foreign>,
    pub open: Option<Place>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Relation {
    pub entity: String,
    pub property: String,
    pub kind: String,
    pub target: String,
}

#[derive(Debug, Default)]
pub(crate) struct Schema {
    pub tables: Vec<Table>,
    pub relations: Vec<Relation>,
    /// Entity files that could not be read: unreadable text, or annotation mappings.
    pub unreadable: Vec<String>,
}

fn mapping(attribute: &AttributeUse) -> Option<&str> {
    attribute.name.strip_prefix(MAPPING)
}

/// The quoted strings of `['a', "b"]`.
fn strings(value: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = value;
    while let Some(at) = rest.find(['\'', '"']) {
        let quote = &rest[at..at + 1];
        let Some(end) = rest[at + 1..].find(quote) else { break };
        found.push(rest[at + 1..at + 1 + end].to_owned());
        rest = &rest[at + 2 + end..];
    }
    found
}

/// The argument lists of each `new X(…)` in an attribute argument such as `indexes: [new ORM\Index(…)]`.
fn constructed(value: &str) -> Vec<&str> {
    value
        .split("new ")
        .skip(1)
        .filter_map(|item| {
            let open = item.find('(')?;
            let close = item.rfind(')')?;
            (close > open).then(|| &item[open + 1..close])
        })
        .collect()
}

fn named<'a>(arguments: &'a str, name: &str) -> Option<String> {
    argument(arguments, name).and_then(string_value)
}

struct Join {
    name: String,
    references: String,
}

/// `name`/`referencedColumnName` of each join column argument list, or the default when there is none.
fn joins<'a>(lists: impl Iterator<Item = &'a str>, default: String) -> Vec<Join> {
    let mut found: Vec<Join> = lists
        .map(|arguments| Join {
            name: named(arguments, "name").unwrap_or_else(|| default.clone()),
            references: named(arguments, "referencedColumnName").unwrap_or_else(|| "id".into()),
        })
        .collect();
    if found.is_empty() {
        found.push(Join { name: default, references: "id".into() });
    }
    found
}

struct Mapped {
    path: String,
    text: String,
    entity: entities::Entity,
    attributes: Vec<AttributeUse>,
}

impl Mapped {
    fn class_attribute(&self, name: &str) -> Option<&AttributeUse> {
        self.attributes.iter().find(|attribute| matches!(attribute.target, Target::Class { .. }) && mapping(attribute) == Some(name))
    }

    fn property_attributes<'a>(&'a self, property: &'a str) -> impl Iterator<Item = &'a AttributeUse> + 'a {
        self.attributes.iter().filter(move |attribute| matches!(&attribute.target, Target::Property { name } if name == property))
    }

    fn arguments(&self, attribute: &AttributeUse) -> &str {
        &self.text[attribute.arguments.clone()]
    }
}

fn place(path: &str, line: u32) -> Option<Place> {
    Some(Place { path: path.to_owned(), line })
}

/// The schema of `files` (path and text of each candidate entity file; `None` when unreadable).
pub(crate) fn build(files: &[(String, Option<String>)], naming: Naming) -> Schema {
    let mut schema = Schema::default();
    let mut mapped = Vec::new();
    for (path, text) in files {
        let Some(text) = text else {
            schema.unreadable.push(path.clone());
            continue;
        };
        match entities::parse(path, text) {
            Some(entity) => mapped.push(Mapped { path: path.clone(), attributes: php_text::attributes(text), text: text.clone(), entity }),
            None if text.contains("@ORM\\Entity") || text.contains("#[ORM\\Entity") || text.contains("Mapping\\Entity") => schema.unreadable.push(path.clone()),
            None => {}
        }
    }
    let table_of: BTreeMap<String, String> = mapped
        .iter()
        .map(|entity| {
            let declared = entity.class_attribute("Table").and_then(|table| named(entity.arguments(table), "name"));
            (entity.entity.class.clone(), declared.unwrap_or_else(|| naming.table(&entity.entity.class)))
        })
        .collect();
    for entity in &mapped {
        tables_of(entity, naming, &table_of, &mut schema);
    }
    schema
}

fn tables_of(mapped: &Mapped, naming: Naming, table_of: &BTreeMap<String, String>, schema: &mut Schema) {
    let entity = &mapped.entity;
    let path = &mapped.path;
    let mut table = Table { name: table_of[&entity.class].clone(), entity: Some(entity.class.clone()), open: place(path, entity.entry.at.line), ..Table::default() };
    let mut column_of: BTreeMap<String, String> = BTreeMap::new();
    let mut primary = Vec::new();

    for field in &entity.fields {
        let column = mapped.property_attributes(&field.name).find(|attribute| mapping(attribute) == Some("Column"));
        let arguments = column.map(|column| mapped.arguments(column)).unwrap_or_default();
        let name = named(arguments, "name").unwrap_or_else(|| naming.column(&field.name));
        let mut kind = field.type_name.clone();
        if let Some(length) = argument(arguments, "length").filter(|length| length.bytes().all(|byte| byte.is_ascii_digit())) {
            kind.push_str(&format!("({length})"));
        }
        if field.nullable {
            kind.push_str(", nullable");
        }
        if argument(arguments, "unique") == Some("true") {
            table.indexes.push(Index { columns: vec![name.clone()], kind: "unique" });
        }
        if field.id {
            primary.push(name.clone());
        }
        column_of.insert(field.name.clone(), name.clone());
        table.columns.push(Column { name, kind, open: place(path, field.entry.at.line) });
    }

    for relation in &entity.relations {
        let Some(target) = relation.target.clone() else { continue };
        if relation.kind == "Embedded" {
            continue;
        }
        schema.relations.push(Relation { entity: entity.class.clone(), property: relation.name.clone(), kind: relation.kind.clone(), target: target.clone() });
        let attributes: Vec<&AttributeUse> = mapped.property_attributes(&relation.name).collect();
        let association = attributes.iter().find(|attribute| mapping(attribute) == Some(relation.kind.as_str()));
        let inverse = association.is_some_and(|attribute| argument(mapped.arguments(attribute), "mappedBy").is_some());
        let join_lists = |name: &str| attributes.iter().filter(move |attribute| mapping(attribute) == Some(name)).map(|attribute| mapped.arguments(attribute)).collect::<Vec<_>>();
        let on = table_of.get(&target).cloned();
        let open = place(path, relation.entry.at.line);
        match relation.kind.as_str() {
            "ManyToOne" | "OneToOne" if !inverse => {
                for join in joins(join_lists("JoinColumn").into_iter(), naming.join_column(&relation.name)) {
                    if relation.kind == "OneToOne" {
                        table.indexes.push(Index { columns: vec![join.name.clone()], kind: "unique" });
                    }
                    column_of.entry(relation.name.clone()).or_insert_with(|| join.name.clone());
                    table.columns.push(Column { name: join.name.clone(), kind: format!("{} → {}", relation.kind, short_name(&target)), open: open.clone() });
                    table.foreigns.push(Foreign { column: join.name, on: on.clone(), references: join.references });
                }
            }
            "ManyToMany" if !inverse => {
                let join_table = attributes.iter().find(|attribute| mapping(attribute) == Some("JoinTable")).map(|attribute| mapped.arguments(attribute)).unwrap_or_default();
                let nested = |key: &str| argument(join_table, key).map(constructed).unwrap_or_default();
                let owner_lists = [join_lists("JoinColumn"), nested("joinColumns")].concat();
                let inverse_lists = [join_lists("InverseJoinColumn"), nested("inverseJoinColumns")].concat();
                let owners = joins(owner_lists.into_iter(), naming.join_key(&entity.class));
                let targets = joins(inverse_lists.into_iter(), naming.join_key(&target));
                let mut link = Table {
                    name: named(join_table, "name").unwrap_or_else(|| naming.join_table(&entity.class, &target)),
                    open: open.clone(),
                    ..Table::default()
                };
                for (joins, references, class) in [(owners, Some(table.name.clone()), &entity.class), (targets, on.clone(), &target)] {
                    for join in joins {
                        link.columns.push(Column { name: join.name.clone(), kind: format!("ManyToMany → {}", short_name(class)), open: open.clone() });
                        link.foreigns.push(Foreign { column: join.name, on: references.clone(), references: join.references });
                    }
                }
                link.indexes.push(Index { columns: link.columns.iter().map(|column| column.name.clone()).collect(), kind: "primary" });
                schema.tables.push(link);
            }
            _ => {}
        }
    }
    if !primary.is_empty() {
        table.indexes.insert(0, Index { columns: primary, kind: "primary" });
    }

    let mut declared = Vec::new();
    for attribute in mapped.attributes.iter().filter(|attribute| matches!(attribute.target, Target::Class { .. })) {
        match mapping(attribute) {
            Some("Index") => declared.push((mapped.arguments(attribute), "index")),
            Some("UniqueConstraint") => declared.push((mapped.arguments(attribute), "unique")),
            Some("Table") => {
                let arguments = mapped.arguments(attribute);
                for (key, kind) in [("indexes", "index"), ("uniqueConstraints", "unique")] {
                    for nested in argument(arguments, key).map(constructed).unwrap_or_default() {
                        declared.push((nested, kind));
                    }
                }
            }
            _ => {}
        }
    }
    for (arguments, kind) in declared {
        let mut columns = argument(arguments, "columns").map(strings).unwrap_or_default();
        for field in argument(arguments, "fields").map(strings).unwrap_or_default() {
            columns.push(column_of.get(&field).cloned().unwrap_or_else(|| naming.column(&field)));
        }
        if !columns.is_empty() {
            table.indexes.push(Index { columns, kind });
        }
    }
    schema.tables.push(table);
}

/// `Version*` migration classes as `(class, path)`, named after their files: Doctrine Migrations
/// requires the class to match the file name.
pub(crate) fn migration_classes(paths: &[String]) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = paths
        .iter()
        .filter_map(|path| {
            let class = path.rsplit('/').next()?.strip_suffix(".php")?;
            (class.starts_with("Version") && class.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')).then(|| (class.to_owned(), path.clone()))
        })
        .collect();
    found.sort();
    found
}

#[derive(Deserialize, Debug, Default, Clone)]
pub(crate) struct LiveIndex {
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub(crate) struct LiveTable {
    pub name: String,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub indexes: Vec<LiveIndex>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub(crate) struct Live {
    #[serde(default)]
    pub tables: Vec<LiveTable>,
    /// Executed migration versions; `None` when the database has no migrations table.
    #[serde(default)]
    pub executed: Option<Vec<String>>,
    #[serde(default)]
    pub connection: String,
    #[serde(default)]
    pub driver: String,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, PartialEq)]
pub(crate) struct Diff {
    pub table: String,
    pub status: &'static str,
    pub details: Vec<String>,
    pub open: Option<Place>,
}

impl Diff {
    pub(crate) fn json(&self, root: &str) -> Value {
        json!({"table":self.table,"status":self.status,"details":self.details,"open":place_json(root, self.open.as_ref())})
    }
}

pub(crate) fn place_json(root: &str, place: Option<&Place>) -> Value {
    place.map_or(Value::Null, |place| {
        let relative = place.path.strip_prefix(root).unwrap_or(&place.path).trim_start_matches('/');
        json!({"path":place.path,"relative":relative,"line":place.line + 1})
    })
}

fn lowered<'a>(items: impl Iterator<Item = &'a String>) -> BTreeSet<String> {
    items.map(|name| name.to_ascii_lowercase()).collect()
}

fn index_keys<'a>(sets: impl Iterator<Item = &'a Vec<String>>) -> BTreeSet<String> {
    sets.filter(|columns| !columns.is_empty()).map(|columns| columns.join("+").to_ascii_lowercase()).collect()
}

fn listed(label: &str, items: &BTreeSet<String>) -> Option<String> {
    (!items.is_empty()).then(|| format!("{label}: {}", items.iter().cloned().collect::<Vec<_>>().join(", ")))
}

/// Pending migrations, then mapped tables missing or different in the database, then unmapped tables.
pub(crate) fn compare(schema: &Schema, migrations: &[(String, String)], live: &Live) -> Vec<Diff> {
    let mut rows = Vec::new();
    let executed: BTreeSet<&str> = live.executed.iter().flatten().map(|version| short_name(version)).collect();
    for (class, path) in migrations {
        if !executed.contains(class.as_str()) {
            rows.push(Diff { table: class.clone(), status: "pending", details: vec!["This migration has not run on this database".into()], open: place(path, 0) });
        }
    }
    for table in &schema.tables {
        let Some(found) = live.tables.iter().find(|found| found.name.eq_ignore_ascii_case(&table.name)) else {
            rows.push(Diff {
                table: table.name.clone(),
                status: "only-migrations",
                details: vec!["The entity mappings define this table but the database does not have it".into()],
                open: table.open.clone(),
            });
            continue;
        };
        let ours = lowered(table.columns.iter().map(|column| &column.name));
        let theirs = lowered(found.columns.iter());
        let missing_columns: BTreeSet<String> = ours.difference(&theirs).cloned().collect();
        let extra_columns: BTreeSet<String> = theirs.difference(&ours).cloned().collect();
        let our_indexes = index_keys(table.indexes.iter().filter(|index| index.kind != "primary").map(|index| &index.columns));
        let their_indexes = index_keys(found.indexes.iter().filter(|index| !index.primary).map(|index| &index.columns));
        let missing_indexes: BTreeSet<String> = our_indexes.difference(&their_indexes).cloned().collect();
        let details: Vec<String> =
            [listed("Missing columns", &missing_columns), listed("Columns only in the database", &extra_columns), listed("Missing indexes", &missing_indexes)].into_iter().flatten().collect();
        if !details.is_empty() {
            rows.push(Diff { table: table.name.clone(), status: "differs", details, open: table.open.clone() });
        }
    }
    let ours: BTreeSet<String> = schema.tables.iter().map(|table| table.name.to_ascii_lowercase()).collect();
    for table in &live.tables {
        let lower = table.name.to_ascii_lowercase();
        if !ours.contains(&lower) && !INTERNAL.contains(&lower.as_str()) {
            rows.push(Diff {
                table: table.name.clone(),
                status: "only-database",
                details: vec!["No entity maps this table; a bundle or raw SQL may have created it".into()],
                open: None,
            });
        }
    }
    rows
}

/// Tables with their keys; solid links for foreign keys, dashed for associations not already joined.
pub(crate) fn diagram(root: &str, schema: &Schema) -> Value {
    let known: BTreeSet<&str> = schema.tables.iter().map(|table| table.name.as_str()).collect();
    let table_of = |class: &str| schema.tables.iter().find(|table| table.entity.as_deref() == Some(class)).map(|table| table.name.as_str());
    let mut tables = Vec::new();
    let mut links = Vec::new();
    let mut joined: BTreeSet<(String, String)> = BTreeSet::new();
    for table in &schema.tables {
        let columns: Vec<Value> = table
            .columns
            .iter()
            .map(|column| {
                let name = &column.name;
                let key = if table.indexes.iter().any(|index| index.kind == "primary" && index.columns.contains(name)) {
                    "pk"
                } else if table.foreigns.iter().any(|foreign| &foreign.column == name) {
                    "fk"
                } else if table.indexes.iter().any(|index| index.kind == "unique" && index.columns == [name.clone()]) {
                    "uk"
                } else {
                    ""
                };
                json!({"name":name,"kind":column.kind,"key":key,"open":place_json(root, column.open.as_ref())})
            })
            .collect();
        let mut foreigns = Vec::new();
        for foreign in &table.foreigns {
            foreigns.push(json!({"column":foreign.column,"on":foreign.on,"references":foreign.references}));
            let Some(target) = foreign.on.as_deref().filter(|target| known.contains(target)) else { continue };
            joined.insert((table.name.clone(), target.to_owned()));
            joined.insert((target.to_owned(), table.name.clone()));
            links.push(json!({"from":table.name,"column":foreign.column,"to":target,"references":foreign.references,"kind":"foreign key","dashed":false}));
        }
        let models: Vec<&str> = table.entity.iter().map(|class| short_name(class)).collect();
        let indexes: Vec<Value> = table.indexes.iter().map(|index| json!({"columns":index.columns,"kind":index.kind})).collect();
        tables.push(json!({"name":table.name,"columns":columns,"models":models,"indexes":indexes,"foreigns":foreigns,"open":place_json(root, table.open.as_ref())}));
    }
    for relation in &schema.relations {
        let (Some(from), Some(to)) = (table_of(&relation.entity), table_of(&relation.target)) else { continue };
        if from == to || !joined.insert((from.to_owned(), to.to_owned())) {
            continue;
        }
        joined.insert((to.to_owned(), from.to_owned()));
        links.push(json!({"from":from,"column":"","to":to,"references":"","kind":format!("{}::${} {}", short_name(&relation.entity), relation.property, relation.kind),"dashed":true}));
    }
    json!({"tables":tables,"links":links})
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const POST: &str = "<?php\nnamespace App\\Entity;\nuse Doctrine\\ORM\\Mapping as ORM;\nuse Doctrine\\Common\\Collections\\Collection;\n\n#[ORM\\Entity]\n#[ORM\\Table(name: 'blog_post', indexes: [new ORM\\Index(columns: ['published_at'], name: 'idx_published')])]\n#[ORM\\UniqueConstraint(fields: ['slugText'])]\nclass BlogPost\n{\n    #[ORM\\Id, ORM\\GeneratedValue, ORM\\Column]\n    private ?int $id = null;\n\n    #[ORM\\Column(length: 120, unique: true)]\n    private string $title;\n\n    #[ORM\\Column]\n    private string $slugText;\n\n    #[ORM\\Column(name: 'published_at', type: 'datetime_immutable', nullable: true)]\n    private ?\\DateTimeImmutable $publishedAt = null;\n\n    #[ORM\\ManyToOne(targetEntity: User::class, inversedBy: 'posts')]\n    private ?User $mainAuthor = null;\n\n    #[ORM\\ManyToOne(targetEntity: User::class)]\n    #[ORM\\JoinColumn(name: 'editor', referencedColumnName: 'uid', nullable: false)]\n    private User $editor;\n\n    /** @var Collection<int, Tag> */\n    #[ORM\\ManyToMany(targetEntity: Tag::class)]\n    private Collection $tags;\n\n    /** @var Collection<int, Comment> */\n    #[ORM\\OneToMany(mappedBy: 'post', targetEntity: Comment::class)]\n    private Collection $comments;\n}\n";
    const USER: &str = "<?php\nnamespace App\\Entity;\nuse Doctrine\\ORM\\Mapping as ORM;\n#[ORM\\Entity]\nclass User\n{\n    #[ORM\\Id, ORM\\Column]\n    private ?int $id = null;\n\n    #[ORM\\OneToOne(targetEntity: Profile::class)]\n    private ?Profile $profile = null;\n}\n";
    const TAG: &str = "<?php\nnamespace App\\Entity;\nuse Doctrine\\ORM\\Mapping as ORM;\n#[ORM\\Entity]\nclass Tag\n{\n    #[ORM\\Id, ORM\\Column]\n    private ?int $id = null;\n\n    #[ORM\\ManyToMany(targetEntity: BlogPost::class, mappedBy: 'tags')]\n    private $posts;\n}\n";
    const COMMENT: &str = "<?php\nnamespace App\\Entity;\nuse Doctrine\\ORM\\Mapping as ORM;\n#[ORM\\Entity]\nclass Comment\n{\n    #[ORM\\Id, ORM\\Column]\n    private ?int $id = null;\n\n    #[ORM\\ManyToOne(targetEntity: BlogPost::class, inversedBy: 'comments')]\n    private ?BlogPost $post = null;\n}\n";
    const ANNOTATED: &str = "<?php\nnamespace App\\Entity;\nuse Doctrine\\ORM\\Mapping as ORM;\n/** @ORM\\Entity */\nclass Legacy {}\n";

    pub(crate) fn files() -> Vec<(String, Option<String>)> {
        [("BlogPost", Some(POST)), ("User", Some(USER)), ("Tag", Some(TAG)), ("Comment", Some(COMMENT)), ("Legacy", Some(ANNOTATED)), ("Latin1", None), ("Repo", Some("<?php\nclass Repo {}\n"))]
            .map(|(name, text)| (format!("/p/src/Entity/{name}.php"), text.map(str::to_owned)))
            .to_vec()
    }

    fn table<'a>(schema: &'a Schema, name: &str) -> &'a Table {
        schema.tables.iter().find(|table| table.name == name).unwrap_or_else(|| panic!("no table {name}"))
    }

    fn names(table: &Table) -> Vec<&str> {
        table.columns.iter().map(|column| column.name.as_str()).collect()
    }

    #[test]
    fn columns_follow_the_mapping_and_the_underscore_naming_strategy() {
        let schema = build(&files(), Naming::NumberAware);
        let post = table(&schema, "blog_post");
        assert_eq!(names(post), ["id", "title", "slug_text", "published_at", "main_author_id", "editor"]);
        assert_eq!(post.columns[1].kind, "string(120)");
        assert_eq!(post.columns[3].kind, "datetime_immutable, nullable");
        assert_eq!(post.columns[4].kind, "ManyToOne → User");
        assert_eq!(post.columns[2].open, Some(Place { path: "/p/src/Entity/BlogPost.php".into(), line: 17 }));
        let foreigns: Vec<_> = post.foreigns.iter().map(|f| (f.column.as_str(), f.on.as_deref(), f.references.as_str())).collect();
        assert_eq!(foreigns, [("main_author_id", Some("user"), "id"), ("editor", Some("user"), "uid")]);
        let indexes: Vec<_> = post.indexes.iter().map(|i| (i.kind, i.columns.join("+"))).collect();
        assert_eq!(indexes, [("primary", "id".to_owned()), ("unique", "title".to_owned()), ("index", "published_at".to_owned()), ("unique", "slug_text".to_owned())]);
        assert_eq!(names(table(&schema, "user")), ["id", "profile_id"]);
        assert_eq!(table(&schema, "user").indexes[1].kind, "unique", "a one-to-one join column is unique");
    }

    #[test]
    fn owning_many_to_many_gets_a_join_table_and_inverse_sides_none() {
        let schema = build(&files(), Naming::NumberAware);
        let link = table(&schema, "blog_post_tag");
        assert_eq!(names(link), ["blog_post_id", "tag_id"]);
        assert_eq!(link.indexes, [Index { columns: vec!["blog_post_id".into(), "tag_id".into()], kind: "primary" }]);
        assert_eq!(link.foreigns.iter().map(|f| f.on.as_deref().unwrap()).collect::<Vec<_>>(), ["blog_post", "tag"]);
        assert_eq!(schema.tables.iter().filter(|table| table.name.contains("tag")).count(), 2, "tag and one join table");
        assert_eq!(names(table(&schema, "tag")), ["id"]);
    }

    #[test]
    fn unreadable_and_annotation_entities_are_listed_others_ignored() {
        let schema = build(&files(), Naming::NumberAware);
        assert_eq!(schema.unreadable, ["/p/src/Entity/Legacy.php", "/p/src/Entity/Latin1.php"]);
        assert!(!schema.tables.iter().any(|table| table.name == "repo"));
    }

    #[test]
    fn naming_strategies() {
        assert_eq!(Naming::NumberAware.table("App\\Entity\\Address2Line"), "address2_line");
        assert_eq!(Naming::Underscore.table("App\\Entity\\Address2Line"), "address2line");
        assert_eq!(Naming::NumberAware.table("HTTPStatus"), "httpstatus");
        assert_eq!(Naming::Default.table("App\\BlogPost"), "BlogPost");
        assert_eq!(Naming::Default.join_table("App\\BlogPost", "App\\Tag"), "blogpost_tag");
        assert_eq!(Naming::Default.join_column("mainAuthor"), "mainAuthor_id");
        assert_eq!(Naming::from_config(None), Naming::NumberAware);
        assert_eq!(Naming::from_config(Some("orm:\n  naming_strategy: doctrine.orm.naming_strategy.underscore_number_aware\n")), Naming::NumberAware);
        assert_eq!(Naming::from_config(Some("orm:\n  naming_strategy: doctrine.orm.naming_strategy.underscore\n")), Naming::Underscore);
        assert_eq!(Naming::from_config(Some("orm:\n  auto_mapping: true\n")), Naming::Default);
        let schema = build(&files(), Naming::Default);
        assert!(schema.tables.iter().any(|table| table.name == "blogpost_tag"));
        assert_eq!(names(table(&schema, "blog_post"))[2], "slugText");
    }

    #[test]
    fn diagram_draws_foreign_keys_solid_and_unjoined_associations_dashed() {
        let schema = build(&files(), Naming::NumberAware);
        let d = diagram("/p", &schema);
        let links: Vec<String> = d["links"].as_array().unwrap().iter().map(|l| format!("{}->{} {} {}", l["from"].as_str().unwrap(), l["to"].as_str().unwrap(), l["kind"].as_str().unwrap(), l["dashed"])).collect();
        assert_eq!(
            links,
            [
                "blog_post_tag->blog_post foreign key false",
                "blog_post_tag->tag foreign key false",
                "blog_post->user foreign key false",
                "blog_post->user foreign key false",
                "comment->blog_post foreign key false",
                "blog_post->tag BlogPost::$tags ManyToMany true",
            ]
        );
        let post = d["tables"].as_array().unwrap().iter().find(|t| t["name"] == "blog_post").unwrap();
        assert_eq!(post["models"], json!(["BlogPost"]));
        assert_eq!(post["columns"][4]["key"], "fk");
        assert_eq!(post["columns"][1]["key"], "uk");
        assert_eq!(post["open"]["relative"], "src/Entity/BlogPost.php");
        assert_eq!(post["columns"][2]["open"]["line"], 18);
    }

    #[test]
    fn compare_lists_pending_missing_differing_and_unmapped_tables() {
        let schema = build(&files(), Naming::NumberAware);
        let migrations = migration_classes(&["/p/migrations/Version2.php", "/p/migrations/Version1.php", "/p/migrations/Helper.php"].map(String::from));
        let live: Live = serde_json::from_value(json!({
            "tables": [
                {"name": "blog_post", "columns": ["id", "title", "slug_text", "published_at", "main_author_id", "editor", "legacy"], "indexes": [{"columns": ["id"], "primary": true}, {"columns": ["title"]}, {"columns": ["published_at"]}]},
                {"name": "user", "columns": ["id", "profile_id"], "indexes": [{"columns": ["profile_id"]}]},
                {"name": "tag", "columns": ["id"]},
                {"name": "comment", "columns": ["id", "post_id"]},
                {"name": "doctrine_migration_versions"},
                {"name": "audit_log"}
            ],
            "executed": ["DoctrineMigrations\\Version1"]
        }))
        .unwrap();
        let rows: Vec<(String, &str, Vec<String>)> = compare(&schema, &migrations, &live).into_iter().map(|row| (row.table, row.status, row.details)).collect();
        assert_eq!(
            rows,
            [
                ("Version2".into(), "pending", vec!["This migration has not run on this database".to_owned()]),
                ("blog_post_tag".into(), "only-migrations", vec!["The entity mappings define this table but the database does not have it".to_owned()]),
                ("blog_post".into(), "differs", vec!["Columns only in the database: legacy".to_owned(), "Missing indexes: slug_text".into()]),
                ("audit_log".into(), "only-database", vec!["No entity maps this table; a bundle or raw SQL may have created it".to_owned()]),
            ]
        );
    }

    #[test]
    fn without_a_migrations_table_every_migration_is_pending() {
        let migrations = migration_classes(&["/p/migrations/Version1.php".to_owned()]);
        let rows = compare(&Schema::default(), &migrations, &Live::default());
        assert_eq!(rows[0].status, "pending");
        assert_eq!(rows[0].json("/p")["open"]["relative"], "migrations/Version1.php");
    }
}
