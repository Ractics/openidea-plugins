//! Bounded, source-derived Eloquent receiver inference. No PHP, filesystem or network calls.
use crate::{
    index::{Entry, Index},
    php,
};
use openidea_plugin::{
    Position, Range,
    providers::{CompletionItem, CompletionKind, Document},
    text,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

#[path = "eloquent_casts.rs"]
mod casts;
#[path = "eloquent_objects.rs"]
mod objects;
#[path = "eloquent_traits.rs"]
mod traits;
pub(crate) use casts::{Enums, cache as cache_enum};
pub(crate) use objects::{Objects, cache as cache_object};
/// Declared enum cases and literal backing values, never runtime evaluation.
pub(crate) fn enum_values(index:&Index,full:&str,owns:&dyn Fn(&str)->bool)->Option<(String,Vec<(String,Option<serde_json::Value>,Entry)>,Option<String>)>{
    let info=index.eloquent_enums.get(&full.trim_start_matches('\\').to_ascii_lowercase()).filter(|info|!info.ambiguous&&owns(&info.path))?;
    Some((info.path.clone(),info.cases.clone()?,info.value_type.clone()))
}
/// Incremental semantic preprocessing; source coverage is unaffected by an
/// unsupported per-model trait/relation graph.
pub(crate) fn finish_model(models:&mut Models,key:&str){
    fn augment(models:&mut Models,key:&str){
        let pending=models.get(key).is_some_and(|model|model.semantic.relation_declarations.through.iter().any(|fact|!model.relations.contains_key(&fact.name))||model.semantic.relation_declarations.annotated.iter().any(|fact|!model.relations.contains_key(&fact.name)));
        if pending&&let Some(mut model)=models.get(key).cloned(){relations::augment(&mut model,models);models.insert(key.into(),model);}
    }
    augment(models,key);traits::finish_one(models,key);augment(models,key);
}
pub(crate) fn finish(models:&mut Models)->bool{relations::finish(models);let complete=traits::finish(models);relations::finish(models);complete}
#[path = "metadata_inference.rs"]
pub(crate) mod metadata_inference;
pub(crate) use metadata_inference::{cache as cache_metadata, finish as finish_metadata};

#[path = "eloquent_helper_facts.rs"]
pub(crate) mod helper_facts;
#[path="eloquent_related_facts.rs"]
pub(crate) mod related_facts;
#[path="eloquent_semantic.rs"]
pub(crate) mod semantic;
pub(crate) use semantic::type_definition_checked;
#[path="eloquent_container.rs"]
mod container;
#[path="eloquent_callbacks.rs"]
mod callbacks;
#[path="eloquent_pipe.rs"]
mod pipe;
pub(crate) use pipe::{Functions,cache as cache_functions};
#[path="eloquent_relations.rs"]
mod relations;
pub(crate) use relations::{BelongsToKeys,belongs_to_key_source,belongs_to_keys};
#[path="package_declared_relations.rs"]
pub(crate) mod package_relations;
#[path="eloquent_accessors.rs"]
mod accessors;
#[path="eloquent_conditionals.rs"]
mod conditionals;
#[path="eloquent_package_queries.rs"]
pub(crate) mod package_queries;

#[path="eloquent_syntax_cache.rs"]
mod syntax_cache;
pub(crate) use syntax_cache::enter as index_syntax_scope;

const MAX_TEXT: usize = 128 * 1024;
const MAX_TOKENS: usize = 8192;
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Name,
    Var,
    String,
    Symbol,
}
#[derive(Clone, Copy, Debug)]
struct Token<'a> {
    value: &'a str,
    start: usize,
    end: usize,
    kind: Kind,
}
struct Source<'a> {
    text: &'a str,
    tokens: Vec<Token<'a>>,
    pairs: std::rc::Rc<BTreeMap<usize, usize>>,
    paths: std::rc::Rc<Vec<Vec<usize>>>,
}
impl<'a> Source<'a> {
    fn new(text: &'a str) -> Option<Self> {
        if text.len()>MAX_TEXT{return None;}
        let hash=match syntax_cache::lookup(text){syntax_cache::Lookup::Hit(source)=>return source,syntax_cache::Lookup::Miss(hash)=>Some(hash),syntax_cache::Lookup::Inactive=>None};
        let source=Self::parse(text);if let Some(hash)=hash{syntax_cache::remember(hash,text,source.as_ref());}source
    }
    fn parse(text: &'a str) -> Option<Self> {
        if text.len() > MAX_TEXT {
            return None;
        }
        let bytes = text.as_bytes();
        let mut tokens = vec![];
        let mut at = 0;
        while at < bytes.len() {
            if tokens.len() >= MAX_TOKENS {
                return None;
            }
            if bytes[at].is_ascii_whitespace() {
                at += 1;
                continue;
            }
            if bytes[at..].starts_with(b"<<<") {
                return None;
            }
            if bytes[at..].starts_with(b"//")
                || bytes[at] == b'#' && bytes.get(at + 1) != Some(&b'[')
            {
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
                continue;
            }
            if bytes[at..].starts_with(b"/*") {
                at += 2;
                while at < bytes.len() && !bytes[at..].starts_with(b"*/") {
                    at += 1;
                }
                if at == bytes.len() {
                    break;
                }
                at += 2;
                continue;
            }
            let start = at;
            let kind;
            if matches!(bytes[at], b'\'' | b'"' | b'`') {
                let quote = bytes[at];
                at += 1;
                while at < bytes.len() {
                    if bytes[at] == b'\\' {
                        at = (at + 2).min(bytes.len());
                    } else if bytes[at] == quote {
                        at += 1;
                        break;
                    } else {
                        at += 1;
                    }
                }
                kind = Kind::String;
            } else if bytes[at] == b'$' {
                at += 1;
                while at < bytes.len() && php::is_identifier(bytes[at]) {
                    at += 1;
                }
                kind = Kind::Var;
            } else if bytes[at].is_ascii_alphabetic() || bytes[at] >= 0x80 || matches!(bytes[at], b'_' | b'\\') {
                at += 1;
                while at < bytes.len() && (php::is_identifier(bytes[at]) || bytes[at] == b'\\') {
                    at += 1;
                }
                kind = Kind::Name;
            } else {
                let size = [
                    "?->", "...", "|>", "->", "::", "=>", "??", "==", "!=", "+=", "-=", "*=", "/=", "++", "--",
                    "&=", "|=",
                ]
                .into_iter()
                .find(|s| text[start..].starts_with(s))
                .map_or_else(
                    || text[start..].chars().next().unwrap().len_utf8(),
                    str::len,
                );
                at += size;
                kind = Kind::Symbol;
            }
            if !text.is_char_boundary(at) {
                return None;
            }
            tokens.push(Token {
                value: &text[start..at],
                start,
                end: at,
                kind,
            });
        }
        let mut stack: Vec<(usize, &str)> = vec![];
        let mut pairs = BTreeMap::new();
        let mut braces = vec![];
        let mut paths = vec![];
        for (i, token) in tokens.iter().enumerate() {
            if token.value == "}" {
                braces.pop()?;
            }
            paths.push(braces.clone());
            match token.value {
                "(" | "[" | "{" => {
                    stack.push((i, token.value));
                    if token.value == "{" {
                        braces.push(i);
                    }
                    if stack.len() > 64 {
                        return None;
                    }
                }
                ")" | "]" | "}" => {
                    let (open, kind) = stack.pop()?;
                    if !matches!((kind, token.value), ("(", ")") | ("[", "]") | ("{", "}")) {
                        return None;
                    }
                    pairs.insert(open, i);
                    pairs.insert(i, open);
                }
                _ => {}
            }
        }
        Some(Self {
            text,
            tokens,
            pairs: std::rc::Rc::new(pairs),
            paths: std::rc::Rc::new(paths),
        })
    }
    fn is(&self, i: usize, value: &str) -> bool {
        self.tokens
            .get(i)
            .is_some_and(|t| t.value.eq_ignore_ascii_case(value))
    }
    fn functions(&self) -> Vec<Function> {
        let mut found = vec![];
        for (i, t) in self.tokens.iter().enumerate() {
            if !t.value.eq_ignore_ascii_case("function") {
                continue;
            }
            let mut at = i + 1;
            if self.is(at, "&") {
                at += 1;
            }
            let name = if self.tokens.get(at).is_some_and(|t| t.kind == Kind::Name) {
                let n = at;
                at += 1;
                Some(n)
            } else {
                None
            };
            if !self.is(at, "(") {
                continue;
            }
            let Some(&close) = self.pairs.get(&at) else {
                continue;
            };
            let mut body = close + 1;
            while body < self.tokens.len() && !self.is(body, "{") && !self.is(body, ";") {
                body += 1;
            }
            if !self.is(body, "{") {
                continue;
            }
            let end = self.pairs.get(&body).copied().unwrap_or(self.tokens.len());
            found.push(Function {
                start: i,
                name,
                params: at + 1..close,
                body,
                end,
            });
        }
        for(i,token)in self.tokens.iter().enumerate().filter(|(_,token)|token.value=="fn"){
            let _=token;
            if !self.is(i+1,"("){continue;}
            let Some(close)=self.pairs.get(&(i+1)).copied()else{continue};
            let mut body=close+1;
            if self.is(body,":"){body+=1;while self.tokens.get(body).is_some_and(|token|token.kind==Kind::Name||matches!(token.value,"?"|"|"|"&")){body+=1;}}
            if !self.is(body,"=>"){continue;}
            let mut end=body+1;
            while end<self.tokens.len()&&!matches!(self.tokens[end].value,","|";"|")"|"]"|"}"){
                if let Some(close)=self.pairs.get(&end).filter(|close|**close>end){end=close+1;}else{end+=1;}
            }
            found.push(Function{start:i,name:None,params:i+2..close,body,end});
        }
        found
    }
}
#[derive(Clone)]
struct Function {
    start: usize,
    name: Option<usize>,
    params: std::ops::Range<usize>,
    body: usize,
    end: usize,
}
#[derive(Default)]
struct Names {
    namespace: String,
    imports: BTreeMap<String, String>,
    class: Option<(String, usize, usize, String)>,
}
impl Names {
    fn parse(source: &Source) -> Option<Self> {
        let t = &source.tokens;
        let namespace_decl=|at:usize|source.is(at,"namespace")&&source.paths[at].is_empty()&&!source.is(at+1,":")&&!source.is(at+1,"=")&&(at==0||!matches!(t[at-1].value,"::"|"->"|"?->"|"function"));
        if t.iter().enumerate()
            .filter(|(at,_)|namespace_decl(*at))
            .count()
            > 1
            || t.iter()
                .enumerate()
                .filter(|(i, t)| {
                    t.value.eq_ignore_ascii_case("class") && !source.is(*i+1,":") && (*i == 0 || !matches!(source.tokens[i-1].value,"::"|"->"|"?->"|"function"))
                })
                .count()
                > 1
        {
            return None;
        }
        let mut result = Self::default();
        let mut i = 0;
        while i < t.len() {
            if namespace_decl(i) {
                if !result.namespace.is_empty() || !source.is(i + 2, ";") {
                    return None;
                }
                result.namespace = t.get(i + 1)?.value.trim_matches('\\').into();
                i += 3;
                continue;
            }
            if ["class", "enum", "interface", "trait"]
                .iter()
                .any(|kind| source.is(i, kind))
                && !source.is(i+1,":")
                && (i == 0 || !matches!(source.tokens[i-1].value,"::"|"->"|"?->"|"function"))
            {
                if t.get(i + 1)?.kind != Kind::Name {
                    return None;
                }
                let mut end = i + 2;
                while end < t.len() && !source.is(end, "{") {
                    end += 1;
                }
                let mut parent = String::new();
                for j in i + 2..end {
                    if source.is(j, "extends") {
                        parent = result.resolve(t.get(j + 1)?.value);
                    }
                }
                let full = result.qualify(t[i + 1].value);
                result.class = Some((full, i, end, parent));
                break;
            }
            if source.is(i, "use") {
                if source.is(i + 1, "function") || source.is(i + 1, "const") {
                    while i < t.len() && !source.is(i, ";") {
                        i += 1;
                    }
                    i += 1;
                    continue;
                }
                i += 1;
                let mut prefix = String::new();
                if source.is(i + 1, "{") {
                    prefix = t.get(i)?.value.into();
                    i += 2;
                }
                loop {
                    let token = t.get(i)?;
                    if token.kind != Kind::Name {
                        return None;
                    }
                    let full = format!("{prefix}{}", token.value)
                        .trim_start_matches('\\')
                        .to_owned();
                    i += 1;
                    let alias = if source.is(i, "as") {
                        i += 1;
                        let name = t.get(i)?.value;
                        i += 1;
                        name
                    } else {
                        token.value.rsplit('\\').next()?
                    };
                    if result
                        .imports
                        .insert(alias.to_ascii_lowercase(), full)
                        .is_some()
                    {
                        return None;
                    }
                    if source.is(i, ",") {
                        i += 1;
                        continue;
                    }
                    if !prefix.is_empty() {
                        if !source.is(i, "}") {
                            return None;
                        }
                        i += 1;
                    }
                    if !source.is(i, ";") {
                        return None;
                    }
                    i += 1;
                    break;
                }
                continue;
            }
            i += 1;
        }
        Some(result)
    }
    fn qualify(&self, name: &str) -> String {
        if self.namespace.is_empty() {
            name.into()
        } else {
            format!("{}\\{name}", self.namespace)
        }
    }
    fn resolve(&self, name: &str) -> String {
        if let Some(full) = name.strip_prefix('\\') {
            return full.into();
        }
        if let Some(relative) = name.strip_prefix("namespace\\") {
            return self.qualify(relative);
        }
        let (head, tail) = name.split_once('\\').unwrap_or((name, ""));
        if let Some(full) = self.imports.get(&head.to_ascii_lowercase()) {
            if tail.is_empty() {
                full.clone()
            } else {
                format!("{full}\\{tail}")
            }
        } else {
            self.qualify(name)
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Relation {
    source_type:Option<String>,
    evidence_paths:Vec<String>,
    target: String,
    kind: String,
    many: bool,
    entry: Entry,
}
#[derive(Clone, Debug)]
pub(crate) struct Model {
    full: String,
    ambiguous: bool,
    path: String,
    parent: String,
    base: bool,
    explicit_table: bool,
    is_trait: bool,
    is_abstract: bool,
    traits: Vec<String>,
    trait_adaptations: bool,
    adaptations: Vec<traits::Adaptation>,
    adapted: bool,
    trait_sources: Vec<String>,
    returns: BTreeMap<String, objects::Method>,
    soft_deletes: bool,
    property_casts: Option<Option<casts::Casts>>,
    method_casts: Option<Option<casts::Casts>>,
    table: Option<String>,
    relations: BTreeMap<String, Relation>,
    scopes: BTreeMap<String, Entry>,
    declared: BTreeSet<String>,
    semantic: semantic::Declared,
}
/// Cached declaration identity for reviewed consumers. Callers must validate
/// ownership and freshness before offering edits; this performs no host reads.
#[derive(Clone,Debug)]
pub(crate) struct ClassIdentity {
    pub full:String,
    pub path:String,
    pub parent:String,
    pub traits:Vec<String>,
    pub is_trait:bool,
    pub is_abstract:bool,
    pub ambiguous:bool,
}
pub(crate) fn class_inventory(models:&Models)->Vec<ClassIdentity>{
    models.values().map(|model|ClassIdentity{full:model.full.clone(),path:model.path.clone(),parent:model.parent.clone(),traits:model.traits.clone(),is_trait:model.is_trait,is_abstract:model.is_abstract,ambiguous:model.ambiguous}).collect()
}
pub(crate) type Models = BTreeMap<String, Model>;
pub(crate) fn cache(models: &mut Models, source: &str, path: &str) {
    if let Some(info) = model(source, path) {
        models
            .entry(info.full.to_ascii_lowercase())
            .and_modify(|old| {
                if old.path != path {
                    old.ambiguous = true;
                }
            })
            .or_insert(info);
    }
}

/// Cheap declared-method graph input for installed-source safety checks. All
/// visibilities and abstract declarations count; trait adaptations are unknown.
pub(crate) fn declared_method_inventory(text:&str,path:&str)->Option<(String,String,Vec<String>,Vec<(String,Entry)>)>{
    let source=Source::new(text)?;let names=Names::parse(&source)?;let(full,_,body,parent)=names.class.as_ref()?;
    if !source.pairs.contains_key(body){return None;}
    let mut traits=Vec::new();let mut methods=Vec::new();let mut seen=BTreeSet::new();
    for(at,token)in source.tokens.iter().enumerate().filter(|(at,_)|source.paths[*at]==vec![*body]){
        if token.value.eq_ignore_ascii_case("use")&&source.tokens.get(at+1).is_some_and(|t|t.kind==Kind::Name){
            let mut next=at+1;loop{
                let name=source.tokens.get(next).filter(|t|t.kind==Kind::Name)?;traits.push(names.resolve(name.value));next+=1;
                if source.is(next,"{"){return None;}if source.is(next,";"){break;}if !source.is(next,","){return None;}next+=1;
            }
        }
        if !token.value.eq_ignore_ascii_case("function")||at>0&&matches!(source.tokens[at-1].value,"::"|"->"|"?->"){continue;}
        let name_at=at+1+usize::from(source.is(at+1,"&"));
        let Some(name)=source.tokens.get(name_at).filter(|t|t.kind==Kind::Name)else{continue};
        if !source.is(name_at+1,"(")||!source.pairs.contains_key(&(name_at+1)){return None;}
        if !seen.insert(name.value.to_ascii_lowercase()){return None;}
        methods.push((name.value.into(),entry(&source,path,name_at)));
    }
    Some((full.clone(),parent.clone(),traits,methods))
}

/// PHP integer syntax without evaluation (decimal, octal, binary, hex and
/// digit separators), bounded to values that PHP represents as signed ints.
pub(crate) fn integer_literal(value:&str)->Option<i64>{
    let value=value.trim();let(negative,value)=if let Some(rest)=value.strip_prefix('-'){(true,rest)}else{(false,value.strip_prefix('+').unwrap_or(value))};
    let value=value.trim_start();let(radix,digits)=if value.starts_with("0x")||value.starts_with("0X"){(16,&value[2..])}else if value.starts_with("0b")||value.starts_with("0B"){(2,&value[2..])}else if value.starts_with("0o")||value.starts_with("0O"){(8,&value[2..])}else if value.starts_with('0')&&value.len()>1{(8,value)}else{(10,value)};
    if digits.is_empty(){return None;}let chars=digits.chars().collect::<Vec<_>>();
    for(at,ch)in chars.iter().enumerate(){if *ch=='_'{if at==0||at+1==chars.len()||!chars[at-1].is_digit(radix)||!chars[at+1].is_digit(radix){return None;}}else if !ch.is_ascii()||!ch.is_digit(radix){return None;}}
    let magnitude=i128::from_str_radix(&digits.replace('_',""),radix).ok()?;if magnitude>i64::MAX as i128{return None;}i64::try_from(if negative{-magnitude}else{magnitude}).ok()
}
/// Literal public own class/enum/interface constants only. No inherited or
/// executable expressions are interpreted; private/protected values are omitted.
pub(crate) fn literal_constants(text:&str)->Option<(String,BTreeMap<String,serde_json::Value>)>{
    let source=Source::new(text)?;let names=Names::parse(&source)?;let(full,_,body,_)=names.class.as_ref()?;
    if !source.pairs.contains_key(body){return None;}let mut result=BTreeMap::new();let mut seen=BTreeSet::new();
    for(at,token)in source.tokens.iter().enumerate().filter(|(at,token)|token.value.eq_ignore_ascii_case("const")&&source.paths[*at]==vec![*body]&&!source.is(*at+1,":")&&!source.is(*at+1,"=")&&(*at==0||!source.is(*at-1,"function")&&!matches!(source.tokens[at-1].value,"::"|"->"|"?->"))){
        let _=token;let begin=(0..at).rev().find(|at|matches!(source.tokens[*at].value,";"|"{"|"}")).map_or(0,|at|at+1);
        let public=!source.tokens[begin..at].iter().any(|token|["private","protected"].iter().any(|modifier|token.value.eq_ignore_ascii_case(modifier)));
        let mut next=at+1;
        loop{
            let equals=(next..source.tokens.len()).find(|at|source.is(*at,"=")||source.is(*at,";"))?;
            if !source.is(equals,"="){break;}let name=source.tokens.get(equals.checked_sub(1)?).filter(|token|token.kind==Kind::Name)?.value;
            if !seen.insert(name.to_owned()){return None;}
            let start=equals+1;let mut end=start;
            while end<source.tokens.len()&&!matches!(source.tokens[end].value,";"|","){
                if let Some(close)=source.pairs.get(&end).filter(|close|**close>end){end=close+1;}else{end+=1;}
            }
            if end==start||end>=source.tokens.len(){return None;}
            let value=if end==start+1&&source.tokens[start].kind==Kind::String{
                let token=&source.tokens[start];php::literal(text,token.start..token.end).and_then(|(value,_)|metadata_inference::decode_literal(value,token.value.as_bytes()[0])).map(serde_json::Value::String)
            }else{integer_literal(&php::mask_comments(&text[source.tokens[start].start..source.tokens[end-1].end])).map(serde_json::Value::from)};
            if public&&let Some(value)=value{result.insert(name.into(),value);}
            if source.is(end,";"){break;}next=end+1;
        }
    }Some((full.clone(),result))
}

/// One direct class property initialized by a decoded literal PHP string.
/// Missing, duplicate, promoted, computed and non-string properties are unknown.
pub(crate) fn class_string_property(text:&str,name:&str)->Option<String>{
    let source=Source::new(text)?;let names=Names::parse(&source)?;let(_,class,body,_)=names.class.as_ref()?;
    if !source.is(*class,"class")||!source.pairs.contains_key(body){return None;}
    let variable=format!("${}",name.trim_start_matches('$'));
    let matches=source.tokens.iter().enumerate().filter(|(at,token)|token.kind==Kind::Var&&token.value==variable&&source.paths[*at]==vec![*body]&&!source.pairs.iter().any(|(open,close)|*open<*at&&*at<*close&&matches!(source.tokens[*open].value,"("|"["))).collect::<Vec<_>>();
    let[(at,_)]=matches.as_slice()else{return None};
    if !source.is(at+1,"=")||!matches!(source.tokens.get(at+3)?.value,";"|","){return None;}
    let value=source.tokens.get(at+2).filter(|token|token.kind==Kind::String)?;
    let(raw,_)=php::literal(text,value.start..value.end)?;metadata_inference::decode_literal(raw,value.value.as_bytes()[0])
}

/// Exact tokens for one own class method, including declaration modifiers,
/// attributes, signature and closed body. Comments/whitespace are excluded.
pub(crate) fn class_method_tokens(text:&str,name:&str)->Option<Vec<String>>{
    let source=Source::new(text)?;let names=Names::parse(&source)?;let(_,class,body,_)=names.class.as_ref()?;
    if !source.is(*class,"class")||!source.pairs.contains_key(body){return None;}
    let functions=source.functions().into_iter().filter(|function|source.paths[function.start]==vec![*body]&&function.name.is_some_and(|at|source.tokens[at].value.eq_ignore_ascii_case(name))).collect::<Vec<_>>();
    let [function]=functions.as_slice()else{return None};
    if !source.is(function.body,"{")||source.pairs.get(&function.body)!=Some(&function.end){return None;}
    let begin=(0..function.start).rev().find(|at|{
        *at==*body||source.paths[*at]==vec![*body]&&matches!(source.tokens[*at].value,";"|"}")&&!source.pairs.iter().any(|(open,close)|*open<*at&&*at<*close&&matches!(source.tokens[*open].value,"("|"["))
    }).map(|at|at+1)?;
    Some(source.tokens[begin..=function.end].iter().map(|token|token.value.into()).collect())
}

/// One named class and its own public instance methods. This is declaration
/// inventory only: no filesystem, inheritance, or controller ancestry inference.
pub(crate) fn controller_declaration(text:&str)->Option<(String,usize,Vec<(String,usize)>)>{
    let source=Source::new(text)?;let names=Names::parse(&source)?;let(full,class,body,_)=names.class.as_ref()?;
    if !source.is(*class,"class")||!source.pairs.contains_key(body){return None;}
    let mut methods=Vec::new();let mut seen=BTreeSet::new();
    for function in source.functions().into_iter().filter(|function|source.paths[function.start]==vec![*body]){
        let Some(at)=function.name else{continue};let name=source.tokens[at].value;
        if !seen.insert(name.to_ascii_lowercase()){return None;}
        let begin=(0..function.start).rev().find(|at|matches!(source.tokens[*at].value,";"|"{"|"}")).map_or(0,|at|at+1);
        if source.tokens[begin..function.start].iter().any(|token|["private","protected","static","abstract"].iter().any(|modifier|token.value.eq_ignore_ascii_case(modifier))){continue;}
        let Some(method)=objects::method(&source,&names,&function,"")else{continue};
        if method.static_method||name.starts_with("__")&&!name.eq_ignore_ascii_case("__invoke"){continue;}
        methods.push((if name.eq_ignore_ascii_case("__invoke"){"__invoke".into()}else{name.into()},source.tokens[at].start));
    }
    Some((full.clone(),source.tokens[class+1].start,methods))
}
pub(crate) fn declaration(text: &str) -> Option<String> {
    let source = Source::new(text)?;
    let names = Names::parse(&source)?;
    Some(names.class?.0)
}
pub(crate) fn model_inventory(models: &Models) -> Vec<(String, String, String)> {
    let mut result = vec![];
    for model in models
        .values()
        .filter(|model| !model.is_trait && !model.ambiguous)
    {
        let mut current = model;
        let mut found = vec![model];
        let mut seen = BTreeSet::new();
        for _ in 0..8 {
            if current.ambiguous
                || current.is_trait
                || !seen.insert(current.full.to_ascii_lowercase())
            {
                break;
            }
            if current.base {
                if let Some(table) = found
                    .iter()
                    .find(|model| model.explicit_table)
                    .copied()
                    .unwrap_or(model)
                    .table
                    .as_ref()
                {
                    result.push((model.full.clone(), model.path.clone(), table.clone()));
                }
                break;
            }
            let Some(parent) = models.get(&current.parent.to_ascii_lowercase()) else {
                break;
            };
            current = parent;
            found.push(parent);
        }
    }
    result
}
/// Eloquent declaration identities independent of a statically known table.
/// Callers retain ownership/freshness and instantiability checks for runtime use.
pub(crate) fn model_identities(models:&Models)->Vec<(String,String)>{
    let mut result=Vec::new();
    for model in models.values().filter(|model|!model.is_trait&&!model.ambiguous){
        let mut current=model;let mut seen=BTreeSet::new();
        for _ in 0..8{
            if current.ambiguous||current.is_trait||!seen.insert(current.full.to_ascii_lowercase()){break;}
            if current.base{result.push((model.full.clone(),model.path.clone()));break;}
            let Some(parent)=models.get(&current.parent.to_ascii_lowercase())else{break};current=parent;
        }
    }result
}
/// A package adapter's literal/source-proven relationship declaration. Every
/// evidence path remains subject to the provider's current ownership predicate.
#[derive(Clone,Debug)]
pub(crate) struct DeclaredRelation {
    pub owner:String,pub owner_path:String,pub name:String,pub target:String,pub kind:String,pub many:bool,pub entry:Entry,pub evidence_paths:Vec<String>,
}
pub(crate) fn apply_declared_relations(models:&mut Models,relations:&[DeclaredRelation]){
    fn inherited_evidence(models:&Models,full:&str,name:&str,fact:&DeclaredRelation,active:&mut BTreeSet<String>,done:&mut BTreeSet<String>,depth:u8)->Option<Vec<String>>{
        if depth>8||done.len()>32{return None;}
        let key=full.to_ascii_lowercase();if done.contains(&key){return Some(vec![]);}
        if !active.insert(key.clone()){return None;}
        let Some(model)=models.get(&key)else{
            active.remove(&key);
            return full.eq_ignore_ascii_case("Illuminate\\Database\\Eloquent\\Model").then_some(vec![]);
        };
        if model.ambiguous||model.trait_adaptations||!model.adaptations.is_empty(){return None;}
        if model.declared.iter().chain(model.semantic.source_methods.iter()).any(|method|method.eq_ignore_ascii_case(name)){
            let same=model.returns.iter().find(|(method,_)|method.eq_ignore_ascii_case(name)).is_some_and(|(_,method)|method.entry.path==fact.entry.path&&method.entry.at==fact.entry.at&&method.entry.width==fact.entry.width);
            if !same{return None;}
        }
        if model.relations.keys().any(|relation|relation.eq_ignore_ascii_case(name)){return None;}
        let mut evidence=vec![model.path.clone()];
        if !model.parent.is_empty(){evidence.extend(inherited_evidence(models,&model.parent,name,fact,active,done,depth+1)?);}
        for trait_ in &model.traits{if models.contains_key(&trait_.to_ascii_lowercase()){evidence.extend(inherited_evidence(models,trait_,name,fact,active,done,depth+1)?);}}
        active.remove(&key);done.insert(key);Some(evidence)
    }
    let mut grouped=BTreeMap::<(String,String),Vec<&DeclaredRelation>>::new();
    for relation in relations{grouped.entry((relation.owner.to_ascii_lowercase(),relation.name.clone())).or_default().push(relation);}
    for((owner,name),facts)in grouped{
        let [fact]=facts.as_slice()else{continue};
        let Some(inherited)=inherited_evidence(models,&owner,&name,fact,&mut BTreeSet::new(),&mut BTreeSet::new(),0)else{continue};
        let Some(model)=models.get_mut(&owner).filter(|model|!model.ambiguous&&model.path==fact.owner_path)else{continue};
        if model.semantic.source_methods.iter().any(|method|method.eq_ignore_ascii_case(&name))||model.relations.contains_key(&name)||!crate::generator_plan::identifier(&name){continue;}
        let mut evidence=fact.evidence_paths.clone();evidence.extend(inherited);evidence.push(fact.owner_path.clone());evidence.sort();evidence.dedup();
        model.relations.insert(name,Relation{target:fact.target.trim_start_matches('\\').into(),kind:fact.kind.clone(),many:fact.many,entry:fact.entry.clone(),source_type:Some(fact.kind.clone()),evidence_paths:evidence});
    }
}

/// Source-backed members for the Models workspace; preserve exact declaration
/// destinations instead of treating related class names as rename anchors.
pub(crate) fn member_inventory(models:&Models)->Vec<(String,String,String,Entry)> {
    let mut result=Vec::new();
    let eligible:BTreeSet<_>=model_inventory(models).into_iter().map(|(full,_,_)|full).collect();
    for model in models.values().filter(|model|eligible.contains(&model.full)) {
        for (name,relation) in &model.relations {
            let mut entry=relation.entry.clone();
            entry.detail=Some(format!("{} → {}",relation.kind,relation.target));
            result.push((model.full.clone(),"Relation".into(),name.clone(),entry));
        }
        for (name,entry) in &model.scopes {result.push((model.full.clone(),"Scope".into(),name.clone(),entry.clone()));}
    }
    result
}

/// Explicit project metadata is an author-supplied model/table override. Keep
/// the distinction from source inference; dynamic table methods still suppress
/// resolver-derived fields unless the caller explicitly elects an override.
pub(crate) fn apply_model_tables(models:&mut Models,metadata:&crate::metadata::Metadata) {
    for model in models.values_mut().filter(|model|!model.is_trait&&!model.ambiguous) {
        if let Some(table)=metadata.model_table(&model.full) {
            model.table=Some(table.into());model.explicit_table=true;
        }
    }
}

/// Current-buffer class ancestry proof for scope transformations. Cached parents
/// must be unambiguous, owned and outside the current file; no PHP executes.
pub(crate) fn scope_model_source(index:&Index,doc:&Document)->bool {
    let Some(project)=openidea_plugin::host::project_of(&doc.path) else{return false};
    if doc.language!="php"||!crate::owns_current_path(&project,&doc.path){return false;}
    let Some(current)=model(&doc.text,&doc.path).filter(|model|!model.is_trait&&!model.ambiguous) else{return false};
    if index.eloquent_models.get(&current.full.to_ascii_lowercase()).is_some_and(|cached|cached.ambiguous||cached.path!=doc.path){return false;}
    let mut current=&current;let mut seen=BTreeSet::new();
    for _ in 0..8 {
        if !seen.insert(current.full.to_ascii_lowercase()){return false;}
        if current.base{return true;}
        let Some(parent)=index.eloquent_models.get(&current.parent.to_ascii_lowercase()).filter(|parent|!parent.is_trait&&!parent.ambiguous&&parent.path!=doc.path&&crate::owns_current_path(&project,&parent.path)) else{return false};
        current=parent;
    }
    false
}
/// Declaration and opening-body offsets without interpreting model properties.
pub(crate) fn class_layout(text:&str)->Option<(usize,usize)> {
    let source=Source::new(text)?;let names=Names::parse(&source)?;
    let (_,start,body,_)=names.class.as_ref()?;if !source.is(*start,"class"){return None;}
    let mut declaration=*start;
    loop {
        if declaration>0&&matches!(source.tokens[declaration-1].value,"final"|"abstract"|"readonly"){declaration-=1;continue;}
        if declaration>0&&source.is(declaration-1,"]") {
            if let Some(open)=source.pairs.iter().find(|(open,close)|**close==declaration-1&&source.is(**open,"[")).map(|(open,_)|*open).filter(|open|*open>0&&source.is(*open-1,"#")){declaration=open-1;continue;}
        }
        break;
    }
    Some((source.tokens[declaration].start,source.tokens[*body].end))
}

/// Exact class offsets and an optional literal fillable array, for reviewed
/// native edits. Dynamic fillable declarations are withheld.
pub(crate) fn edit_layout(text:&str)->Option<(usize,usize,Option<std::ops::Range<usize>>)> {
    let source=Source::new(text)?;let names=Names::parse(&source)?;
    let (_,start,body,_)=names.class.as_ref()?;
    if !source.is(*start,"class") {return None;}
    let mut fillable=None;
    for (i,t) in source.tokens.iter().enumerate() {
        if t.value=="$fillable" && source.paths[i]==vec![*body] {
            if fillable.is_some() || !source.is(i+1,"=") || !source.is(i+2,"["){return None;}
            let end=*source.pairs.get(&(i+2))?;
            fillable=Some(source.tokens[i+2].start..source.tokens[end].end);
        }
    }
    let mut declaration=*start;
    loop {
        if declaration>0 && matches!(source.tokens[declaration-1].value,"final"|"abstract"|"readonly") {declaration-=1;continue;}
        if declaration>0 && source.is(declaration-1,"]") {
            let opening=source.pairs.iter().find(|(open,close)|**close==declaration-1&&source.is(**open,"[")).map(|(open,_)|*open);
            if let Some(open)=opening.filter(|open|*open>0&&source.is(*open-1,"#")){declaration=open-1;continue;}
        }
        break;
    }
    Some((source.tokens[declaration].start,source.tokens[*body].end,fillable))
}

fn entry(source: &Source, path: &str, token: usize) -> Entry {
    let t = &source.tokens[token];
    Entry {
        path: path.into(),
        at: text::position(source.text, t.start),
        width: t.value.encode_utf16().count() as u32,
        detail: None,
    }
}
const RELATIONS: &[(&str, bool)] = &[
    ("hasOne", false),
    ("belongsTo", false),
    ("hasMany", true),
    ("belongsToMany", true),
    ("hasOneThrough", false),
    ("hasManyThrough", true),
    ("morphOne", false),
    ("morphMany", true),
    ("morphToMany", true),
    ("morphedByMany", true),
];
pub(crate) fn model(text: &str, path: &str) -> Option<Model> {
    let source = Source::new(text)?;
    let names = Names::parse(&source)?;
    let (full, start, body, parent) = names.class.as_ref()?;
    if !source.is(*start, "class") && !source.is(*start, "trait") {
        return None;
    }
    let base = [
        "Illuminate\\Database\\Eloquent\\Model",
        "Illuminate\\Foundation\\Auth\\User",
        "Illuminate\\Database\\Eloquent\\Relations\\Pivot",
        "Illuminate\\Database\\Eloquent\\Relations\\MorphPivot",
    ]
    .iter()
    .any(|base| parent.eq_ignore_ascii_case(base));
    let end = source
        .pairs
        .get(body)
        .copied()
        .unwrap_or(source.tokens.len());
    // Multiple classes / conditional declarations are outside this source subset.
    if source
        .tokens
        .iter()
        .enumerate()
        .any(|(i, t)| i > end && t.value.eq_ignore_ascii_case("class"))
    {
        return None;
    }
    let mut result = Model {
        semantic: semantic::declared(&source,&names,path),
        full: full.clone(),
        ambiguous: false,
        path: path.into(),
        parent: parent.clone(),
        base,
        explicit_table: false,
        is_trait: source.is(*start, "trait"),
        is_abstract: ((*start).saturating_sub(2)..*start).any(|at|source.is(at,"abstract")),
        traits: Vec::new(),
        trait_adaptations: false,
        adaptations: Vec::new(),
        adapted: false,
        trait_sources: Vec::new(),
        returns: BTreeMap::new(),
        soft_deletes: false,
        property_casts: None,
        method_casts: None,
        table: Some(crate::index::table_name(full.rsplit('\\').next()?)),
        relations: BTreeMap::new(),
        scopes: BTreeMap::new(),
        declared: BTreeSet::new(),
    };
    for i in 0..source.tokens.len() {
        if source.is(i, "use") && source.paths[i] == vec![*body] {
            let mut at = i + 1;
            loop {
                let Some(token) = source
                    .tokens
                    .get(at)
                    .filter(|token| token.kind == Kind::Name)
                else {
                    result.trait_adaptations = true;
                    break;
                };
                let full = names.resolve(token.value);
                if full == "Illuminate\\Database\\Eloquent\\SoftDeletes" {
                    result.soft_deletes = true;
                }
                result.traits.push(full);
                at += 1;
                if source.is(at, ",") {
                    at += 1;
                    continue;
                }
                if source.is(at, "{") {
                    if let Some(adaptations) = traits::parse(&source, &names, at, path) {
                        result.adaptations.extend(adaptations);
                    } else {
                        result.trait_adaptations = true;
                    }
                } else if !source.is(at, ";") {
                    result.trait_adaptations = true;
                }
                break;
            }
        }
    }
    for (i, t) in source.tokens.iter().enumerate() {
        if t.value == "$casts" && source.paths[i] == vec![*body] {
            result.property_casts = Some(
                if source.is(i + 1, "=")
                    && source
                        .pairs
                        .get(&(i + 2))
                        .is_some_and(|close| source.is(close + 1, ";"))
                {
                    casts::array(&source, &names, i + 2, path)
                } else {
                    None
                },
            );
        }
    }
    for (i, t) in source.tokens.iter().enumerate() {
        if t.value == "$table" && source.paths[i] == vec![*body] {
            result.explicit_table = true;
            if !source.is(i + 1, "=") || !source.is(i + 3, ";") {
                result.table = None;
                continue;
            }
            result.table = source
                .tokens
                .get(i + 2)
                .filter(|t| t.kind == Kind::String)
                .and_then(|t| php::literal(text, t.start..t.end))
                .map(|(s, _)| s.to_owned());
        }
    }
    for function in source.functions() {
        if source.paths[function.start] != vec![*body] {
            continue;
        }
        let Some(at) = function.name else {
            continue;
        };
        let name = source.tokens[at].value;
        result.declared.insert(name.into());
        if let Some(method) = objects::method(&source, &names, &function, path) {
            result.returns.insert(name.into(), method);
        }
        if let Some(rest) = name
            .strip_prefix("scope")
            .filter(|s| s.starts_with(|c: char| c.is_ascii_uppercase()))
        {
            let scope = format!("{}{}", rest[..1].to_ascii_lowercase(), &rest[1..]);
            result.scopes.insert(scope, entry(&source, path, at));
        } else {
            let start = (0..function.start)
                .rev()
                .find(|&i| source.is(i, "}") || source.is(i, ";") || source.is(i, "{"))
                .map_or(0, |i| i + 1);
            for i in start..function.start {
                if source.is(i, "#")
                    && source.is(i + 1, "[")
                    && source.tokens.get(i + 2).is_some_and(|t| {
                        names.resolve(t.value)
                            == "Illuminate\\Database\\Eloquent\\Attributes\\Scope"
                    })
                {
                    result.scopes.insert(name.into(), entry(&source, path, at));
                }
            }
        }
        let b = function.body + 1;
        if name == "casts" {
            result.method_casts = Some(
                if source.is(b, "return")
                    && source
                        .pairs
                        .get(&(b + 1))
                        .is_some_and(|close| source.is(close + 1, ";") && close + 2 == function.end)
                {
                    casts::array(&source, &names, b + 1, path)
                } else {
                    None
                },
            );
        }
        if !source.is(b, "return")
            || !source.is(b + 1, "$this")
            || !source.is(b + 2, "->")
            || !source.is(b + 4, "(")
        {
            continue;
        }
        let factory = source.tokens.get(b + 3)?.value;
        let Some((_, many)) = RELATIONS.iter().find(|(kind, _)| *kind == factory) else {
            continue;
        };
        let target = source.tokens.get(b + 5)?;
        if target.value == "static" {
            continue;
        }
        if target.kind != Kind::Name || !source.is(b + 6, "::") || !source.is(b + 7, "class") {
            continue;
        }
        let Some(&close) = source.pairs.get(&(b + 4)) else {
            continue;
        };
        let mut tail = close + 1;
        while source.is(tail, "->") {
            let Some(method) = source.tokens.get(tail + 1) else {
                break;
            };
            if ![
                "withDefault",
                "withPivot",
                "withTimestamps",
                "as",
                "where",
                "whereIn",
                "whereNull",
                "whereNotNull",
                "orderBy",
                "latest",
                "oldest",
                "select",
                "withTrashed",
                "wherePivot",
                "wherePivotIn",
            ]
            .contains(&method.value)
                || !source.is(tail + 2, "(")
            {
                break;
            }
            let Some(&end) = source.pairs.get(&(tail + 2)) else {
                break;
            };
            tail = end + 1;
        }
        if !source.is(tail, ";") || tail + 1 != function.end {
            continue;
        }

        if close > function.end || ![",", ")"].contains(&source.tokens.get(b + 8)?.value) {
            continue;
        }
        result.relations.insert(
            name.into(),
            Relation {
                source_type:None,
                evidence_paths:Vec::new(),
                target: match target.value {
                    "self" | "static" => full.clone(),
                    "parent" => parent.clone(),
                    _ => names.resolve(target.value),
                },
                kind: factory.into(),
                many: *many,
                entry: entry(&source, path, at),
            },
        );
    }
    relations::apply_native(&mut result);
    if result.declared.contains("getTable") {
        result.table = None;
    }
    result
        .relations
        .retain(|_, r| r.source_type.is_some()||!result.declared.contains(&r.kind));
    Some(result)
}

#[derive(Clone, Debug, PartialEq)]
enum Shape {
    Static,
    Model,
    Query,
    Collection,
    MappedCollection(String),
    JoinClause(String),
    Factory { class:String, many:Option<bool> },
    Facade,
    HigherOrder(String,String),
    Optional(Box<Shape>),
    Conditional{target:Box<Shape>,truth:Option<bool>,capture:bool,invert:bool},
    Relation(String),
    Enum,
    Object,
}
#[derive(Clone, Debug)]
struct Typed {
    model: String,
    shape: Shape,
}
struct Context<'a> {
    source: Source<'a>,
    names: Names,
    arrow: usize,
    start: usize,
    end: usize,
    typed: &'a str,
    member: &'a str,
}
fn context<'a>(doc: &'a Document, at: Position) -> Option<Context<'a>> {
    let cursor = text::offset(&doc.text, at)?;
    let source = Source::new(&doc.text)?;
    let names = Names::parse(&source)?;
    let t = &source.tokens;
    let word = t
        .iter()
        .position(|t| t.kind == Kind::Name && t.start <= cursor && cursor <= t.end);
    let (arrow, start, end, typed, member) = if let Some(word) = word {
        if t[word].value.contains('\\') {
            return None;
        }
        (
            word.checked_sub(1)?,
            t[word].start,
            t[word].end,
            &doc.text[t[word].start..cursor],
            t[word].value,
        )
    } else {
        let last = t.iter().rposition(|t| t.end <= cursor)?;
        if !doc.text[t[last].end..cursor].trim().is_empty() {
            return None;
        }
        (last, cursor, cursor, "", "")
    };
    if !matches!(t.get(arrow)?.value, "->" | "?->" | "::") {
        return None;
    }
    Some(Context {
        source,
        names,
        arrow,
        start,
        end,
        typed,
        member,
    })
}
struct Resolver<'a, 'b> {
    context: &'a Context<'b>,
    index: &'a Index,
    overlay: Option<Model>,
    object_overlay: Option<objects::Object>,
    path: &'a str,
    owns: &'a dyn Fn(&str) -> bool,
    owner_cache: RefCell<BTreeMap<String, bool>>,
}
impl<'a, 'b> Resolver<'a, 'b> {
    fn owned(&self, path: &str) -> bool {
        if let Some(owned) = self.owner_cache.borrow().get(path).copied() {
            return owned;
        }
        let owned = (self.owns)(path);
        self.owner_cache.borrow_mut().insert(path.into(), owned);
        owned
    }
    fn raw(&self, full: &str) -> Option<&Model> {
        if self
            .index
            .eloquent_models
            .get(&full.to_ascii_lowercase())
            .is_some_and(|m| m.ambiguous)
        {
            return None;
        }
        if self
            .overlay
            .as_ref()
            .is_some_and(|m| m.full.eq_ignore_ascii_case(full))
        {
            return self.overlay.as_ref().filter(|model| {
                self.owned(&model.path) && model.trait_sources.iter().all(|path| self.owned(path))
            });
        }
        let model = self.index.eloquent_models.get(&full.to_ascii_lowercase())?;
        (model.path != self.path
            && self.owned(&model.path)
            && model.trait_sources.iter().all(|path| self.owned(path)))
        .then_some(model)
    }
    fn lineage(&self, full: &str) -> Option<Vec<&Model>> {
        let mut found = vec![];
        let mut current = self.raw(full)?;
        let mut seen = BTreeSet::new();
        for _ in 0..8 {
            if !seen.insert(current.full.to_ascii_lowercase()) {
                return None;
            }
            if current.is_trait {
                return None;
            }
            found.push(current);
            if !current.adapted {
                if let Some(traits) = self.traits(current) {
                    found.extend(traits);
                }
            }
            if current.base {
                return Some(found);
            }
            current = self.raw(&current.parent)?;
        }
        None
    }
    fn traits(&self, model: &Model) -> Option<Vec<&Model>> {
        fn walk<'a>(
            resolver: &'a Resolver<'_, '_>,
            model: &Model,
            found: &mut Vec<&'a Model>,
            active: &mut BTreeSet<String>,
            depth: u8,
        ) -> Option<()> {
            if model.adapted {
                return Some(());
            }
            if depth >= 8 || model.trait_adaptations || !model.adaptations.is_empty() {
                return None;
            }
            for full in &model.traits {
                let Some(info) = resolver.raw(full) else {
                    continue;
                };
                if !info.is_trait || !active.insert(full.to_ascii_lowercase()) {
                    return None;
                }
                if !found.iter().any(|old| old.full == info.full) {
                    if found.len() >= 32 {
                        return None;
                    }
                    found.push(info);
                    walk(resolver, info, found, active, depth + 1)?;
                }
                active.remove(&full.to_ascii_lowercase());
            }
            Some(())
        }
        let mut found = vec![];
        walk(self, model, &mut found, &mut BTreeSet::new(), 0)?;
        let mut declared = BTreeSet::new();
        for info in &found {
            for name in &info.declared {
                if !model.declared.contains(name) && !declared.insert(name) {
                    return None;
                }
            }
        }
        Some(found)
    }
    fn object(&self, full: &str) -> Option<&objects::Object> {
        if self.index.eloquent_objects.get(&full.to_ascii_lowercase()).is_some_and(|object|object.ambiguous||self.object_overlay.as_ref().is_some_and(|overlay|overlay.full.eq_ignore_ascii_case(full)&&overlay.path!=object.path)){return None;}
        if let Some(object)=self.object_overlay.as_ref().filter(|object|object.full.eq_ignore_ascii_case(full)) {return (self.owned(&object.path)&&object.metadata_sources.iter().all(|path|self.owned(path))).then_some(object);}
        let object = self
            .index
            .eloquent_objects
            .get(&full.to_ascii_lowercase())?;
        (!object.ambiguous && object.path != self.path && self.owned(&object.path) && object.metadata_sources.iter().all(|path|self.owned(path)))
            .then_some(object)
    }
    fn returned(&self, method: &objects::Method, receiver: &Typed) -> Option<Typed> {
        if !self.owned(&method.entry.path) || method.static_method != (receiver.shape == Shape::Static) {
            return None;
        }
        self.return_target(&method.target, receiver)
    }
    fn return_target(&self, target: &str, receiver: &Typed) -> Option<Typed> {
        if target.contains('|') {
            return None;
        }
        let target_count=target.split('&').count();
        let mut concrete = None;
        for target in target.split('&') {
            let full = if target == "static" {
                receiver.model.as_str()
            } else {
                target
            };
            let shape = if self.get(full).is_some() {
                Some(Shape::Model)
            } else if self.enumeration(full).is_some() {
                Some(Shape::Enum)
            } else if let Some(object) = self.object(full) {
                if object.is_interface && target_count > 1 {
                    None
                } else {
                    Some(Shape::Object)
                }
            } else {
                return None;
            };
            if let Some(shape) = shape {
                if concrete.is_some() {
                    return None;
                }
                concrete = Some(Typed {
                    model: full.into(),
                    shape,
                });
            }
        }
        concrete
    }
    fn casts(&self, full: &str) -> Option<casts::Casts> {
        let lineage = self.lineage(full)?;
        if lineage.iter().any(|class|class.declared.iter().any(|name|["getCasts","getAttribute","getAttributeValue","castAttribute"].iter().any(|candidate|name.eq_ignore_ascii_case(candidate)))){return None;}
        let property = lineage.iter().find_map(|m| m.property_casts.as_ref());
        let method = lineage.iter().find_map(|m| m.method_casts.as_ref());
        if property.is_some_and(Option::is_none) || method.is_some_and(Option::is_none) {
            return None;
        }
        let mut result = property
            .and_then(Option::as_ref)
            .cloned()
            .unwrap_or_default();
        result.extend(method.and_then(Option::as_ref).cloned().unwrap_or_default());
        Some(result)
    }
    fn counts(&self, full: &str) -> BTreeMap<String, Entry> {
        let mut result = BTreeMap::new();
        if self.declared(full, "withCount") {
            return result;
        }
        let source = &self.context.source;
        let Some((_, start)) = self.expr(self.context.arrow, 0) else {
            return result;
        };
        let relations = self.relations(full);
        for at in start..self.context.arrow {
            if !source.is(at, "withCount")
                || at == 0
                || !source.is(at + 1, "(")
                || source.paths[at] != source.paths[self.context.arrow]
            {
                continue;
            }
            let Some((receiver, _)) = self.expr(at - 1, 0) else {
                continue;
            };
            if !receiver.model.eq_ignore_ascii_case(full) {
                continue;
            }
            let Some(&close) = source
                .pairs
                .get(&(at + 1))
                .filter(|close| **close < self.context.arrow)
            else {
                continue;
            };
            let stable = (close + 1..self.context.arrow)
                .filter(|&i| {
                    source.paths[i] == source.paths[self.context.arrow]
                        && matches!(source.tokens[i].value, "->" | "?->")
                })
                .all(|i| {
                    source.is(i + 2, "(")
                        && source.tokens.get(i + 1).is_some_and(|token| {
                            [
                                "where",
                                "whereIn",
                                "whereNull",
                                "whereNotNull",
                                "orderBy",
                                "latest",
                                "oldest",
                                "limit",
                                "take",
                                "skip",
                                "offset",
                                "with",
                                "withCount",
                                "first",
                                "firstOrFail",
                                "sole",
                                "get",
                                "all",
                                "find",
                                "findOrFail",
                                "last",
                            ]
                            .contains(&token.value)
                        })
                });
            if !stable {
                continue;
            }
            let mut values = vec![];
            if let (Some(first), Some(last)) = (
                source.tokens.get(at + 2),
                source.tokens.get(close.wrapping_sub(1)),
            ) {
                if let Some((value, _)) = php::literal(source.text, first.start..last.end) {
                    values.push(value.to_owned());
                } else if first.value == "[" {
                    for item in php::items(source.text, first.start) {
                        if let Some((value, _)) =
                            php::literal(source.text, item.key.unwrap_or(item.value))
                        {
                            values.push(value.to_owned());
                        }
                    }
                }
            }
            for value in values.into_iter().take(64) {
                let (relation, alias) = value
                    .split_once(" as ")
                    .map_or((value.as_str(), None), |(name, alias)| (name, Some(alias)));
                let Some(info) = relations.get(relation) else {
                    continue;
                };
                let name = if let Some(alias) = alias {
                    alias.to_owned()
                } else {
                    let chars: Vec<_> = relation.chars().collect();
                    let mut snake = String::new();
                    for (i, &ch) in chars.iter().enumerate() {
                        if ch.is_ascii_uppercase()
                            && i > 0
                            && (chars[i - 1].is_ascii_lowercase()
                                || chars
                                    .get(i + 1)
                                    .is_some_and(|next| next.is_ascii_lowercase()))
                        {
                            snake.push('_');
                        }
                        snake.push(ch.to_ascii_lowercase());
                    }
                    format!("{snake}_count")
                };
                if name.len() <= 128
                    && name
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
                    && name
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                {
                    if result.len() < 64 {
                        result.insert(name, info.entry.clone());
                    }
                }
            }
        }
        result
    }
    fn cast(&self, full: &str, name: &str) -> Option<casts::Cast> {
        self.casts(full)?.get(name).cloned()
    }
    fn enumeration(&self, full: &str) -> Option<&casts::EnumInfo> {
        let info = self.index.eloquent_enums.get(&full.to_ascii_lowercase())?;
        (!info.ambiguous && self.owned(&info.path)).then_some(info)
    }
    fn get(&self, full: &str) -> Option<&Model> {
        self.lineage(full)?;
        self.raw(full)
    }
    fn relation(&self, full: &str, name: &str) -> Option<&Relation> {
        for model in self.lineage(full)? {
            if model.declared.contains(name) {
                let relation = model.relations.get(name)?;
                return ((relation.source_type.is_some()||!self.declared(full, &relation.kind))&&relation.evidence_paths.iter().all(|path|self.owned(path))).then_some(relation);
            }
        }
        self.external_relations(full).get(name).copied()
    }
    fn relations(&self, full: &str) -> BTreeMap<&str, &Relation> {
        let mut result = self.external_relations(full);
        let mut blocked = BTreeSet::new();
        for model in self.lineage(full).unwrap_or_default() {
            for (name, relation) in &model.relations {
                if !blocked.contains(name.as_str()) && (relation.source_type.is_some()||!self.declared(full, &relation.kind))&&relation.evidence_paths.iter().all(|path|self.owned(path)) {
                    result.insert(name.as_str(), relation);
                }
            }
            for name in &model.declared {if !blocked.contains(name.as_str())&&!model.relations.contains_key(name.as_str()) {result.remove(name.as_str());}}
            blocked.extend(model.declared.iter().map(String::as_str));
        }
        result
    }
    fn scopes(&self, full: &str) -> BTreeMap<&str, &Entry> {
        let mut result = BTreeMap::new();
        let mut blocked = BTreeSet::new();
        for model in self.lineage(full).unwrap_or_default() {
            for (name, entry) in &model.scopes {
                if !blocked.contains(name.as_str()) {
                    result.entry(name.as_str()).or_insert(entry);
                }
            }
            blocked.extend(model.declared.iter().map(String::as_str));
        }
        result
    }
    fn declared(&self, full: &str, name: &str) -> bool {
        self.lineage(full)
            .unwrap_or_default()
            .iter()
            .any(|model| model.declared.iter().any(|declared|declared.eq_ignore_ascii_case(name)))
    }
    fn table(&self, full: &str) -> Option<&str> {
        self.get(full)?;
        if let Some(table)=self.index.metadata.model_table(full){return Some(table);}
        let lineage = self.lineage(full)?;
        if lineage.iter().any(|m|m.declared.iter().any(|method|method.eq_ignore_ascii_case("getTable"))) {
            return None;
        }
        lineage
            .iter()
            .find(|m| m.explicit_table)
            .copied()
            .or_else(|| lineage.first().copied())?
            .table
            .as_deref()
    }
    fn class(&self, name: &str) -> Option<String> {
        let name = if matches!(name, "self" | "static") {
            self.context.names.class.as_ref()?.0.clone()
        } else {
            self.context.names.resolve(name)
        };
        self.get(&name).map(|m| m.full.clone()).or_else(||self.object(&name).map(|o|o.full.clone()))
    }
    fn expr(&self, end: usize, depth: u8) -> Option<(Typed, usize)> {
        if depth > 16 || end == 0 {
            return None;
        }
        if let Some(result)=self.pipe_expr(end,depth){return Some(result);}
        let s = &self.context.source;
        let t = &s.tokens;
        let last = end - 1;
        if s.is(last, ")") {
            let open = *s.pairs.get(&last)?;
            if s.is(open+1,"...")&&open+2==last{return None;}
            if open >= 2
                && t[open - 1].kind == Kind::Name
                && matches!(t[open - 2].value, "->" | "?->" | "::")
            {
                let (receiver, start) = self.expr(open - 2, depth + 1)?;
                if matches!(receiver.shape,Shape::Static|Shape::Facade) != (t[open - 2].value == "::") {
                    return None;
                }
                return Some((
                    self.call(receiver, t[open - 1].value, open + 1, last)?,
                    start,
                ));
            }
            if open>0&&t[open-1].kind==Kind::Name&&let Some(typed)=self.optional_target(open,last,depth){return Some((typed,open-1));}
            if open>0&&t[open-1].kind==Kind::Name&&let Some(typed)=self.helper_target(open,last){return Some((typed,open-1));}
            if open >= 2 && s.is(open - 2, "new") && t[open - 1].kind == Kind::Name {
                return Some((
                    Typed {
                        model: self.class(t[open - 1].value)?,
                        shape: if self.get(&self.class(t[open - 1].value)?).is_some(){Shape::Model}else{Shape::Object},
                    },
                    open - 2,
                ));
            }
            // A failed function/dynamic call is not a parenthesized identity.
            if open>0&&!(open>=3&&s.is(open-1,"php")&&s.is(open-2,"?")&&s.is(open-3,"<"))&&(matches!(t[open-1].kind,Kind::Name|Kind::Var)&&!["return","yield","echo","print"].contains(&t[open-1].value)||matches!(t[open-1].value,")"|"]")){return None;}
            let (receiver, start) = self.expr(last, depth + 1)?;
            if start != open + 1 {
                return None;
            }
            return Some((receiver, open));
        }
        if t[last].kind == Kind::Name && last >= 2 && matches!(t[last - 1].value, "->" | "?->") {
            let (mut receiver, start) = self.expr(last - 1, depth + 1)?;
            while let Shape::Optional(target)=&receiver.shape{receiver.shape=*target.clone();}
            if matches!(receiver.shape,Shape::Conditional{..}){return None;}
            if matches!(receiver.shape,Shape::Collection|Shape::MappedCollection(_))&&["each","map","flatMap","filter","reject"].contains(&t[last].value){
                if ["map","flatMap"].contains(&t[last].value)&&!self.native_collection_operation(&receiver,t[last].value){return None;}
                return Some((Typed{shape:Shape::HigherOrder(t[last].value.into(),self.collection_class(&receiver)?),..receiver},start));
            }
            if let Shape::HigherOrder(operation,physical)=&receiver.shape{
                let relation=self.relation(&receiver.model,t[last].value);
                if operation=="map"{let relation=relation.filter(|relation|!relation.many)?;return Some((Typed{model:self.get(&relation.target)?.full.clone(),shape:Shape::MappedCollection(physical.clone())},start));}
                if operation=="flatMap"{let relation=relation.filter(|relation|relation.many)?;let physical=if self.subclass(physical,"Illuminate\\Database\\Eloquent\\Collection"){"Illuminate\\Support\\Collection".into()}else{physical.clone()};return Some((Typed{model:self.get(&relation.target)?.full.clone(),shape:Shape::MappedCollection(physical)},start));}
                let known=relation.is_some()||self.cast(&receiver.model,t[last].value).is_some()||self.table(&receiver.model).and_then(|table|self.index.columns.get(table)).is_some_and(|columns|columns.contains_key(t[last].value));
                return known.then(||(Typed{shape:Shape::MappedCollection(physical.clone()),..receiver},start));
            }
            if receiver.shape != Shape::Model {
                return None;
            }
            if let Some(kind)=self.accessor_type(&receiver.model,t[last].value){return self.accessor_result(&kind?,&receiver).map(|typed|(typed,start));}
            if let Some(cast) = self.cast(&receiver.model, t[last].value) {
                if let Some(info) = cast
                    .target
                    .as_deref()
                    .and_then(|target| self.enumeration(target))
                {
                    return Some((
                        Typed {
                            model: info.full.clone(),
                            shape: Shape::Enum,
                        },
                        start,
                    ));
                }
                if let Some(caster) = cast
                    .target
                    .as_deref()
                    .and_then(|target| self.object(target))
                {
                    let target = caster.cast_target.as_deref()?;
                    return Some((self.return_target(target, &receiver)?, start));
                }
                return None;
            }
            let relation = self.relation(&receiver.model, t[last].value)?;
            let target = self.get(&relation.target)?;
            return Some((
                Typed {
                    model: target.full.clone(),
                    shape: if relation.many {
                        Shape::Collection
                    } else {
                        Shape::Model
                    },
                },
                start,
            ));
        }
        if t[last].kind == Kind::Name {
            if let Some(full)=self.facade_target(&self.context.names.resolve(t[last].value)){return Some((Typed{model:full,shape:Shape::Facade},last));}
            let full = self.class(t[last].value)?;
            if last > 0 && s.is(last - 1, "new") {
                return Some((
                    Typed {
                        shape: if self.get(&full).is_some(){Shape::Model}else{Shape::Object},
                        model: full,
                    },
                    last - 1,
                ));
            }
            return Some((
                Typed {
                    model: full,
                    shape: Shape::Static,
                },
                last,
            ));
        }
        if t[last].kind == Kind::Var {
            return Some((self.binding(last, depth + 1)?, last));
        }
        None
    }
    fn binding(&self, at: usize, depth: u8) -> Option<Typed> {
        if depth > 16 {
            return None;
        }
        let s = &self.context.source;
        let t = &s.tokens;
        let name = t[at].value;
        let functions = s.functions();
        let function = functions
            .iter()
            .filter(|f| f.body < at && at < f.end)
            .max_by_key(|f| f.body);
        if name == "$this" {
            let function = function?;
            // Anonymous functions and static methods do not prove an instance receiver.
            if function.name.is_none()
                || (function.start.saturating_sub(3)..function.start).any(|i| s.is(i, "static"))
            {
                return None;
            }
            let full = &self.context.names.class.as_ref()?.0;
            return if let Some(model)=self.get(full){Some(Typed{model:model.full.clone(),shape:Shape::Model})}else{self.object(full).map(|object|Typed{model:object.full.clone(),shape:Shape::Object})};
        }
        let start = function.map_or(0, |f| f.body + 1);
        // Arrow-function captures and foreach/destructuring aliases are not inferred.
        let mut candidate = None;
        for i in start..at {
            if s.is(i, "fn") && !(i..at).any(|j| s.is(j, ";")) {
                return None;
            }
            if t[i].kind != Kind::Var || t[i].value != name {
                continue;
            }
            if i > 0 && matches!(t[i - 1].value, "->" | "?->") {
                continue;
            }
            if s.is(i + 1, "=") {
                if s.paths[i] != s.paths[at] {
                    return None;
                }
                let mut end = i + 2;
                while end < at {
                    if s.is(end, ";") {
                        break;
                    }
                    if let Some(close) = s.pairs.get(&end).filter(|&&close| close > end) {
                        end = close + 1;
                    } else {
                        end += 1;
                    }
                }
                if end >= at || !s.is(end, ";") {
                    return None;
                }
                let (typed, first) = self.expr(end, depth + 1)?;
                if first != i + 2 || typed.shape == Shape::Static {
                    return None;
                }
                candidate = Some(typed);
            } else if matches!(
                t.get(i + 1).map(|t| t.value),
                Some("+=" | "-=" | "*=" | "/=" | "++" | "--" | "]" | "=>")
            ) || i > 0 && matches!(t[i - 1].value, "as" | "&" | "++" | "--")
                || i >= 2 && s.is(i - 1, "(") && s.is(i - 2, "unset")
            {
                return None;
            }
        }
        if candidate.is_some() {
            return candidate;
        }
        let f = function?;
        if let Some(typed)=self.container_parameter(f,name){return typed;}
        if let Some(typed)=self.callback_binding(f,name){return Some(typed);}
        let params = &t[f.params.clone()];
        let index = params
            .iter()
            .position(|p| p.kind == Kind::Var && p.value == name)?;
        let mut before = &params[..index];
        if before.last().is_some_and(|t| t.value == "&") {
            before = &before[..before.len() - 1];
        }
        let ty = before.last()?;
        if ty.kind != Kind::Name {
            return None;
        }
        let previous = before.get(before.len().wrapping_sub(2)).map(|t| t.value);
        if previous.is_some_and(|v| ![",", "?"].contains(&v)) {
            return None;
        }
        let mut full=self.class(ty.value)?;
        if f.name.is_some_and(|at|s.is(at,"__construct"))&&let Some((owner,_,_,_))=&self.context.names.class&&let Some(target)=self.container_target(&full,Some(owner)){full=target;}
        Some(Typed {
            shape: if self.get(&full).is_some(){Shape::Model}else{Shape::Object},
            model: full,
        })
    }
    fn id_shape(&self, from: usize, to: usize) -> Option<Shape> {
        let source = &self.context.source;
        let tokens = &source.tokens;
        let mut end = from;
        while end < to {
            if source.is(end, ",") {
                break;
            }
            if let Some(close) = source.pairs.get(&end).filter(|&&close| close > end) {
                end = close + 1;
            } else {
                end += 1;
            }
        }
        if from >= end {
            return None;
        }
        if source.is(from, "[") && source.pairs.get(&from) == Some(&(end - 1)) {
            return Some(Shape::Collection);
        }
        if end == from + 1 && tokens[from].kind == Kind::String {
            return Some(Shape::Model);
        }
        let literal = &source.text[tokens[from].start..tokens[end - 1].end];
        if literal.parse::<i64>().is_ok() {
            return Some(Shape::Model);
        }
        if end != from + 1 || tokens[from].kind != Kind::Var {
            return None;
        }
        let name = tokens[from].value;
        let functions = source.functions();
        let function = functions
            .iter()
            .filter(|f| f.body < from && from < f.end)
            .max_by_key(|f| f.body)?;
        for i in function.body + 1..from {
            if tokens[i].value == name
                && (source.is(i + 1, "=")
                    || matches!(
                        tokens.get(i + 1).map(|t| t.value),
                        Some("+=" | "-=" | "*=" | "/=" | "++" | "--" | "]" | "=>")
                    )
                    || i > 0 && matches!(tokens[i - 1].value, "as" | "&" | "++" | "--")
                    || i >= 2 && source.is(i - 1, "(") && source.is(i - 2, "unset"))
            {
                return None;
            }
        }
        let params = &tokens[function.params.clone()];
        let at = params
            .iter()
            .position(|t| t.kind == Kind::Var && t.value == name)?;
        let kind = params.get(at.checked_sub(1)?)?.value;
        if at >= 2 && ![",", "?"].contains(&params[at - 2].value) {
            return None;
        }
        match kind {
            "int" | "string" => Some(Shape::Model),
            "array" => Some(Shape::Collection),
            _ => None,
        }
    }
    fn call(&self, receiver: Typed, name: &str, from: usize, to: usize) -> Option<Typed> {
        if receiver.shape == Shape::Enum {
            return None;
        }
        if matches!(receiver.shape,Shape::JoinClause(_)){return self.package_join_call(&receiver,name);}
        if let Some(result)=self.proxy_call(&receiver,name,from,to){return result;}
        if let Some(result)=self.conditional_call(&receiver,name,from,to){return result;}
        if receiver.shape==Shape::Facade{let typed=Typed{shape:if self.get(&receiver.model).is_some(){Shape::Model}else{Shape::Object},..receiver};return self.call(typed,name,from,to);}
        if let Shape::HigherOrder(operation,physical)=&receiver.shape{
            let inner=Typed{model:receiver.model.clone(),shape:Shape::Model};
            if operation=="map"{let result=self.call(inner,name,from,to)?;return (result.shape==Shape::Model).then(||Typed{shape:Shape::MappedCollection(physical.clone()),..result});}
            if operation=="flatMap"{let result=self.call(inner,name,from,to)?;let physical=if self.subclass(physical,"Illuminate\\Database\\Eloquent\\Collection"){"Illuminate\\Support\\Collection".into()}else{physical.clone()};return matches!(result.shape,Shape::Collection|Shape::MappedCollection(_)).then(||Typed{shape:Shape::MappedCollection(physical),..result});}
            let known=["save","delete","update","refresh","load","loadMissing"].contains(&name)||self.lineage(&receiver.model)?.iter().find(|class|class.declared.contains(name)).and_then(|class|class.returns.get(name)).is_some_and(|method|!method.static_method&&self.owned(&method.entry.path));
            return known.then(||Typed{shape:Shape::MappedCollection(physical.clone()),..receiver});
        }

        if let Shape::Factory{class,many}= &receiver.shape {
            let shape=match name {
                "count"=>Shape::Factory{class:class.clone(),many:self.factory_count(from,to)},
                "create"|"make"|"createQuietly"=>if many.as_ref().copied()?{Shape::Collection}else{Shape::Model},
                "createOne"|"makeOne"|"createOneQuietly"=>Shape::Model,
                "createMany"|"createManyQuietly"=>Shape::Collection,
                "state"|"sequence"|"for"|"has"|"recycle"|"afterMaking"|"afterCreating"|"withoutParents"=>receiver.shape.clone(),
                _=>return self.custom_call(&receiver,name),
            };
            return Some(Typed{shape,..receiver});
        }
        if receiver.shape==Shape::Static&&name=="new"&&self.subclass(&receiver.model,"Illuminate\\Database\\Eloquent\\Factories\\Factory") {
            return Some(Typed{model:self.factory_model(&receiver.model)?,shape:Shape::Factory{class:receiver.model,many:Some(false)}});
        }
        if matches!(receiver.shape,Shape::Static|Shape::Model)&&name=="factory"&&self.native_factory_method(&receiver.model) {
            let factory=self.model_factory(&receiver.model)?;
            let model=self.factory_model(&factory.full)?;
            return Some(Typed{model,shape:Shape::Factory{class:factory.full,many:self.factory_count(from,to)}});
        }
        if let Some(result)=self.mapped_collection(&receiver,name,from,to){return result;}
        if matches!(receiver.shape,Shape::Query|Shape::Collection|Shape::MappedCollection(_)|Shape::Relation(_))&&let Some(result)=self.custom_call(&receiver,name){return Some(result);}
        if let Some(result)=self.package_call(&receiver,name){return Some(result);}
        if name=="newModelInstance"&&self.custom_class(&receiver).is_some_and(|class|self.object_method(&class,name).is_some()){return None;}
        if receiver.shape == Shape::Object || receiver.shape == Shape::Static && self.get(&receiver.model).is_none() {
            self.object(&receiver.model)?;
            let method = self.object_method(&receiver.model,name)?;
            return self.returned(method, &receiver);
        }
        self.get(&receiver.model)?;
        if matches!(receiver.shape,Shape::MappedCollection(_))&&["find","load","loadMissing"].contains(&name)&&self.collection_class(&receiver).is_none_or(|class|!self.subclass(&class,"Illuminate\\Database\\Eloquent\\Collection")){return None;}
        if ["find", "findOrFail"].contains(&name)
            && (!matches!(receiver.shape,Shape::Model|Shape::Static)||!self.declared(&receiver.model, name))
            && matches!(
                receiver.shape,
                Shape::Static | Shape::Query | Shape::Relation(_) | Shape::Collection | Shape::MappedCollection(_)
            )
        {
            return Some(Typed {
                shape: self.id_shape(from, to)?,
                ..receiver
            });
        }
        if receiver.shape == Shape::Model
            && let Some(relation) = self.relation(&receiver.model, name)
        {
            return Some(Typed {
                model: self.get(&relation.target)?.full.clone(),
                shape: Shape::Relation(relation.kind.clone()),
            });
        }
        if matches!(
            receiver.shape,
            Shape::Static | Shape::Model | Shape::Query | Shape::Relation(_)
        ) && !(receiver.shape==Shape::Model&&self.declared(&receiver.model,"__call")) && self.scopes(&receiver.model).keys().any(|candidate|candidate.eq_ignore_ascii_case(name))
        {
            return Some(Typed {
                shape: if matches!(receiver.shape,Shape::Static|Shape::Model) {
                    Shape::Query
                } else {
                    receiver.shape
                },
                ..receiver
            });
        }
        if receiver.shape==Shape::Model&&name=="newCollection"{
            if self.declared(&receiver.model,name)&&self.lineage(&receiver.model)?.iter().find(|class|class.declared.contains(name)).and_then(|class|class.returns.get(name)).is_none(){return None;}
            if self.lineage(&receiver.model)?.iter().any(|class|self.construction_override(class,"newCollection"))&&self.custom_class(&Typed{model:receiver.model.clone(),shape:Shape::Collection}).is_none(){return None;}
            let args=container::arguments(&self.context.source,from.checked_sub(1)?)?;
            if args.len()>1{return None;}
            if let Some(arg)=args.first(){
                let source=&self.context.source;if !source.is(arg.start,"[")||source.pairs.get(&arg.start)!=Some(&(arg.end-1)){return None;}
                for item in php::items(source.text,source.tokens[arg.start].start){
                    let end=source.tokens.iter().position(|token|token.end==item.value.end)?+1;
                    let (value,start)=self.expr(end,1)?;
                    if source.tokens[start].start!=item.value.start||value.shape!=Shape::Model||!value.model.eq_ignore_ascii_case(&receiver.model){return None;}
                }
            }
            return Some(Typed{shape:Shape::Collection,..receiver});
        }
        if matches!(receiver.shape,Shape::Model|Shape::Static)&&self.declared(&receiver.model, name) {
            let method = self
                .lineage(&receiver.model)?
                .into_iter()
                .find_map(|model|model.declared.iter().find(|candidate|candidate.eq_ignore_ascii_case(name)).map(|name|(model,name)))?;
            let method=method.0.returns.get(method.1)?;
            return self.returned(method, &receiver);
        }
        if receiver.shape==Shape::Model{
            if ["newQuery","newModelQuery","newQueryWithoutScopes","newQueryWithoutRelationships","query"].contains(&name){return Some(Typed{shape:Shape::Query,..receiver});}
            let forwarded=QUERY.iter().any(|(method,_)|*method==name)||["first","firstOrFail","sole","get","all","find","findOrFail","findOrNew","findMany","create","firstOrCreate","updateOrCreate","make","newModelInstance"].contains(&name);
            if forwarded&&!self.declared(&receiver.model,"__call"){return self.call(Typed{shape:Shape::Query,..receiver},name,from,to);}
        }
        if let Shape::Relation(kind) = &receiver.shape {
            if ["belongsToMany", "morphToMany", "morphedByMany"].contains(&kind.as_str())
                && [
                    "wherePivot",
                    "wherePivotIn",
                    "wherePivotNull",
                    "withPivot",
                    "withTimestamps",
                    "as",
                ]
                .contains(&name)
            {
                return Some(receiver);
            }
            if name == "getQuery" {
                return Some(Typed {
                    shape: Shape::Query,
                    ..receiver
                });
            }
            if name == "createMany"
                && ["hasMany", "hasOne", "morphMany", "morphOne"].contains(&kind.as_str())
            {
                return Some(Typed {
                    shape: Shape::Collection,
                    ..receiver
                });
            }
        }
        if ["withTrashed", "onlyTrashed", "withoutTrashed"].contains(&name)
            && !self
                .lineage(&receiver.model)?
                .iter()
                .any(|m| m.soft_deletes)
        {
            return None;
        }
        let shape = match &receiver.shape {
            Shape::Static | Shape::Query | Shape::Relation(_) => match name {
                "find" | "findOrFail" | "findOrNew" | "first" | "firstOrFail" | "sole"
                | "create" | "firstOrCreate" | "updateOrCreate" | "make" | "newModelInstance" => Shape::Model,
                "get" | "all" | "findMany" => Shape::Collection,
                "query" | "newQuery" => Shape::Query,
                name if QUERY.iter().any(|(n, _)| *n == name) => {
                    if receiver.shape == Shape::Static {
                        Shape::Query
                    } else {
                        receiver.shape.clone()
                    }
                }
                _ => return None,
            },
            Shape::Collection | Shape::MappedCollection(_) => match name {
                "first" | "last" | "sole" | "find" => Shape::Model,
                name if COLLECTION_KEEP.contains(&name) => receiver.shape.clone(),
                _ => return None,
            },
            Shape::Model => match name {
                "fresh" | "refresh" | "replicate" | "load" | "loadMissing" | "newInstance" | "newFromBuilder" => Shape::Model,
                _ => return None,
            },
            Shape::Enum | Shape::Object | Shape::Factory{..} | Shape::Facade | Shape::HigherOrder(..) | Shape::Optional(_) | Shape::Conditional{..} | Shape::JoinClause(_) => return None,
        };
        Some(Typed { shape, ..receiver })
    }
}
const QUERY: &[(&str, &str)] = &[
    ("from", "Select a query source"),
    ("union", "Combine query results"),
    ("unionAll", "Combine all query results"),
    ("join", "Join a table"),
    ("leftJoin", "Join a table on the left"),
    ("rightJoin", "Join a table on the right"),
    ("crossJoin", "Cross join a table"),
    ("where", "Add a where clause"),
    ("when", "Apply a callback when a condition is true"),
    ("unless", "Apply a callback when a condition is false"),
    ("whereIn", "Match a set of values"),
    ("whereNotIn", "Exclude a set of values"),
    ("whereNull", "Match null values"),
    ("whereNotNull", "Exclude null values"),
    ("whereHas", "Constrain a relationship"),
    ("with", "Eager load relationships"),
    ("withOnly", "Replace eager loads"),
    ("withWhereHas", "Constrain and eager load a relationship"),
    ("orWhereHas", "Constrain an alternative relationship"),
    ("whereDoesntHave", "Exclude matching relationships"),
    ("orWhereDoesntHave", "Exclude alternative matching relationships"),
    ("without", "Exclude eager loads"),
    ("withCount", "Add relationship counts"),
    ("orderBy", "Order query results"),
    ("latest", "Order newest first"),
    ("oldest", "Order oldest first"),
    ("limit", "Limit result count"),
    ("take", "Limit result count"),
    ("skip", "Skip results"),
    ("offset", "Set result offset"),
    ("select", "Select columns"),
    ("addSelect", "Add selected columns"),
    ("distinct", "Select distinct values"),
    ("withTrashed", "Include soft-deleted models"),
    ("onlyTrashed", "Select soft-deleted models"),
    ("withoutTrashed", "Exclude soft-deleted models"),
];
const COLLECTION_KEEP: &[&str] = &[
    "filter",
    "reject",
    "values",
    "reverse",
    "sortBy",
    "sortByDesc",
    "take",
    "skip",
    "shuffle",
    "unique",
    "where",
    "whereIn",
    "whereNotNull",
    "each",
    "load",
    "loadMissing",
];
fn receiver<'a, 'b>(resolver: &Resolver<'a, 'b>) -> Option<Typed> {
    let ctx = resolver.context;
    let (typed, _) = resolver.expr(ctx.arrow, 0)?;
    if matches!(typed.shape,Shape::Static|Shape::Facade) != (ctx.source.tokens[ctx.arrow].value == "::") {
        return None;
    }
    Some(typed)
}
/// Presentation-only localization. The semantic provider remains cached/pure;
/// Main calls this after inference before publishing completion items.
pub(crate) fn localize_completions(items:&mut [CompletionItem]){
    for item in items{
        if let Some(detail)=item.detail.clone(){
            if let Some((name,description))=QUERY.iter().find(|(name,_)|item.filter_text.as_deref()==Some(*name))&&let Some(suffix)=detail.strip_prefix(&format!("{description} · ")).filter(|suffix|suffix.starts_with("Builder<")){
                item.detail=Some(format!("{} · {suffix}",crate::ui_text::t(&format!("eloquent.query.{name}"),description)));
            }else if let Some(class)=detail.strip_prefix("Value object · "){
                item.detail=Some(crate::ui_text::format("eloquent.detail.valueObject","Value object · {class}",&[("class",class)]));
            }else if let Some(model)=detail.strip_suffix(" · integer (withCount)"){
                item.detail=Some(crate::ui_text::format("eloquent.detail.count","{model} · integer (withCount)",&[("model",model)]));
            }else if let Some(value)=detail.strip_suffix(" (declared cast)")&&let Some((model,cast))=value.split_once(" · "){
                item.detail=Some(crate::ui_text::format("eloquent.detail.declaredCast","{model} · {cast} (declared cast)",&[("model",model),("cast",cast)]));
            }else if let Some(model)=detail.strip_prefix("Higher-order collection proxy · "){
                item.detail=Some(crate::ui_text::format("eloquent.detail.higherOrder","Higher-order collection proxy · {model}",&[("model",model)]));
            }else if let Some(class)=detail.strip_prefix("Declared/mixin property · "){
                item.detail=Some(crate::ui_text::format("eloquent.detail.mixinProperty","Declared/mixin property · {class}",&[("class",class)]));
            }
        }
        if let Some(class)=item.documentation.as_deref().and_then(|doc|doc.strip_prefix("Constructor parameters forwarded from `")).and_then(|doc|doc.strip_suffix("`.")){
            item.documentation=Some(crate::ui_text::format("eloquent.detail.constructorForwarded","Constructor parameters forwarded from `{class}`.",&[("class",class)]));
        }
    }
}
fn method_item(name: &str, detail: String, ctx: &Context) -> CompletionItem {
    let mut item = CompletionItem::new(&format!("{name}()"), CompletionKind::Method);
    item.filter_text = Some(name.into());
    item.detail = Some(detail);
    item.replace = Some(Range::new(
        text::position(ctx.source.text, ctx.start),
        text::position(ctx.source.text, ctx.end),
    ));
    if ctx
        .source
        .tokens
        .iter()
        .find(|token| token.start >= ctx.end)
        .is_some_and(|token| token.value == "(")
    {
        item.insert_text = name.into();
    } else {
        item.insert_text = format!("{name}($0)");
        item.snippet = true;
    }
    item
}
#[cfg(test)]
fn completion(index: &Index, doc: &Document, at: Position) -> Vec<CompletionItem> {
    completion_checked(index, doc, at, &|_| true)
}
pub(crate) fn completion_checked(
    index: &Index,
    doc: &Document,
    at: Position,
    owns: &dyn Fn(&str) -> bool,
) -> Vec<CompletionItem> {
    let Some(ctx) = context(doc, at) else {
        return vec![];
    };
    let resolver = Resolver {
        context: &ctx,
        index,
        overlay: model(&doc.text, &doc.path).map(|mut model| {
            let _ = traits::compose(&mut model, &index.eloquent_models, &mut BTreeSet::new(), 0);
            relations::augment(&mut model,&index.eloquent_models);
            model
        }),
        object_overlay:objects::current(&doc.text,&doc.path),
        path: &doc.path,
        owns,
        owner_cache: RefCell::new(BTreeMap::new()),
    };
    let Some(mut typed) = receiver(&resolver) else {
        return vec![];
    };
    while let Shape::Optional(target)|Shape::Conditional{target,..}=&typed.shape{typed.shape=*target.clone();}
    if matches!(typed.shape,Shape::HigherOrder(..)){typed.shape=Shape::Model;}
    if let Shape::Factory{class,..}=&typed.shape {
        let mut items=BTreeMap::new();
        for name in ["count","create","createOne","createMany","createQuietly","make","makeOne","state","sequence","for","has","recycle","afterMaking","afterCreating"] {items.insert(name.to_owned(),method_item(name,format!("Factory<{}> · {class}",typed.model),&ctx));}
        if let Some(object)=resolver.object(class){for(name,method)in resolver.object_methods(&object.full){if !method.static_method&&resolver.owned(&method.entry.path){items.insert(name.to_owned(),method_item(name,format!("Declared factory method · {class}"),&ctx));}}}
        return items.into_values().filter(|item|item.filter_text.as_deref().unwrap_or(&item.label).starts_with(ctx.typed)).collect();
    }
    if matches!(typed.shape,Shape::Object|Shape::Facade) || typed.shape == Shape::Static && resolver.get(&typed.model).is_none() {
        let Some(info) = resolver.object(&typed.model) else {
            return vec![];
        };
        let mut items = vec![];
        for name in info.properties.keys().filter(|_|typed.shape==Shape::Object) {
            let mut item = CompletionItem::new(name, CompletionKind::Property);
            item.detail = Some(format!("Value object · {}", info.full));
            item.replace = Some(Range::new(
                text::position(&doc.text, ctx.start),
                text::position(&doc.text, ctx.end),
            ));
            items.push(item);
        }
        for (name,method) in resolver.object_methods(&typed.model).into_iter().filter(|(_,m)|m.static_method==(typed.shape==Shape::Static)) {
            let mut item=method_item(name,format!("Value object · {}",info.full),&ctx);
            if let Some(signature)=index.metadata.forwarded.get(&(info.full.to_ascii_lowercase(),name.to_ascii_lowercase())) {
                item.detail=Some(format!("{}({}) → {}",name,signature.parameters.join(", "),signature.target));
                item.documentation=Some(format!("Constructor parameters forwarded from `{}`.",info.full));
                if item.snippet {let parameters=signature.parameters.iter().enumerate().map(|(i,p)|{let name=p.split('$').nth(1).unwrap_or("argument").split(|c:char|!c.is_ascii_alphanumeric()&&c!='_').next().unwrap_or("argument");format!("${{{}:{}}}",i+1,name)}).collect::<Vec<_>>().join(", ");item.insert_text=format!("{name}({parameters})$0");}
            }
            let _=method;
            items.push(item);
        }
        return items
            .into_iter()
            .filter(|item| {
                item.filter_text
                    .as_deref()
                    .unwrap_or(&item.label)
                    .to_ascii_lowercase().starts_with(&ctx.typed.to_ascii_lowercase())
            })
            .collect();
    }
    if typed.shape == Shape::Enum {
        let Some(info) = resolver.enumeration(&typed.model) else {
            return vec![];
        };
        let mut items = vec![];
        for (name, kind) in [
            ("name", Some("string")),
            ("value", info.value_type.as_deref()),
        ] {
            if let Some(kind) = kind {
                let mut item = CompletionItem::new(name, CompletionKind::Property);
                item.detail = Some(format!("{} · {kind}", info.full));
                item.replace = Some(Range::new(
                    text::position(&doc.text, ctx.start),
                    text::position(&doc.text, ctx.end),
                ));
                items.push(item);
            }
        }
        for name in info.methods.keys() {
            items.push(method_item(name, format!("Enum · {}", info.full), &ctx));
        }
        return items
            .into_iter()
            .filter(|item| {
                item.filter_text
                    .as_deref()
                    .unwrap_or(&item.label)
                    .to_ascii_lowercase().starts_with(&ctx.typed.to_ascii_lowercase())
            })
            .collect();
    }
    if let Shape::JoinClause(_)=&typed.shape{
        return resolver.package_join_methods(&typed).into_iter().map(|(name,entry)|method_item(&name,entry.path,&ctx)).filter(|item|item.filter_text.as_deref().unwrap_or(&item.label).to_ascii_lowercase().starts_with(&ctx.typed.to_ascii_lowercase())).collect();
    }
    if resolver.get(&typed.model).is_none() {
        return vec![];
    }
    let mut items = BTreeMap::new();
    let short = typed.model.rsplit('\\').next().unwrap_or(&typed.model);
    if let Some(full)=resolver.custom_class(&typed)&&let Some(object)=resolver.object(&full){for(name,method)in resolver.object_methods(&object.full){if !method.static_method&&resolver.owned(&method.entry.path){items.insert(format!("{name}()"),method_item(name,format!("Declared custom receiver · {full}"),&ctx));}}}
    if matches!(typed.shape,Shape::Static|Shape::Model)&&resolver.native_factory_method(&typed.model)&&resolver.model_factory(&typed.model).is_some(){items.insert("factory()".into(),method_item("factory",format!("Resolved factory · {short}"),&ctx));}
    if typed.shape == Shape::Model {
        for (name, _) in resolver.counts(&typed.model) {
            let mut item = CompletionItem::new(&name, CompletionKind::Field);
            item.detail = Some(format!("{short} · integer (withCount)"));
            item.replace = Some(Range::new(
                text::position(&doc.text, ctx.start),
                text::position(&doc.text, ctx.end),
            ));
            items.insert(name, item);
        }
        let casts = resolver.casts(&typed.model).unwrap_or_default();
        if let Some(columns) = resolver
            .table(&typed.model)
            .and_then(|table| index.columns.get(table))
        {
            for (name, column) in columns {
                if !resolver.owned(&column.path) {
                    continue;
                }
                let mut item = CompletionItem::new(name, CompletionKind::Field);
                item.detail = Some(if let Some(cast) = casts.get(name) {
                    format!(
                        "{short} · {} → {} (cast)",
                        column.detail.as_deref().unwrap_or("column"),
                        cast.label
                    )
                } else {
                    format!(
                        "{short} · {} (migration)",
                        column.detail.as_deref().unwrap_or("column")
                    )
                });
                item.replace = Some(Range::new(
                    text::position(&doc.text, ctx.start),
                    text::position(&doc.text, ctx.end),
                ));
                items.insert(name.clone(), item);
            }
        }
        for (name, cast) in &casts {
            if !items.contains_key(name) {
                let mut item = CompletionItem::new(name, CompletionKind::Field);
                item.detail = Some(format!("{short} · {} (declared cast)", cast.label));
                item.replace = Some(Range::new(
                    text::position(&doc.text, ctx.start),
                    text::position(&doc.text, ctx.end),
                ));
                items.insert(name.clone(), item);
            }
        }
        if let Some(accessors)=resolver.accessor_contracts(&typed.model){for(name,accessor)in accessors{
            let mut item=CompletionItem::new(&name,CompletionKind::Property);
            item.detail=Some(format!("{short} · {}",accessor.read_type.as_deref().or(accessor.write_type.as_deref()).unwrap_or("accessor")));
            item.replace=Some(Range::new(text::position(&doc.text,ctx.start),text::position(&doc.text,ctx.end)));
            items.insert(name,item);
        }}
        for (name, relation) in resolver.relations(&typed.model) {
            let Some(target) = resolver.get(&relation.target) else {
                continue;
            };
            let mut item = CompletionItem::new(name, CompletionKind::Property);
            item.detail = Some(format!(
                "{} → {}{}",
                relation.kind,
                if relation.many { "Collection<" } else { "" },
                if relation.many {
                    format!("{}>", target.full)
                } else {
                    target.full.clone()
                }
            ));
            item.replace = Some(Range::new(
                text::position(&doc.text, ctx.start),
                text::position(&doc.text, ctx.end),
            ));
            items.entry(name.to_owned()).or_insert(item);
            items.insert(
                format!("{name}()"),
                method_item(name, format!("{}<{}>", relation.kind, target.full), &ctx),
            );
        }
        for name in [
            "save",
            "delete",
            "update",
            "fresh",
            "refresh",
            "replicate",
            "load",
            "loadMissing",
            "toArray",
            "newCollection",
            "newInstance",
            "newFromBuilder",
        ] {
            if !resolver.declared(&typed.model, name) {
                items.insert(
                    format!("{name}()"),
                    method_item(name, format!("Eloquent model · {short}"), &ctx),
                );
            }
        }
    }
    if matches!(
        typed.shape,
        Shape::Static | Shape::Query | Shape::Relation(_)
    ) {
        for(name,method)in resolver.package_methods(&typed.model){
            if typed.shape!=Shape::Static||!resolver.declared(&typed.model,name){items.entry(format!("{name}()")).or_insert_with(||method_item(name,method.entry.path.clone(),&ctx));}
        }
        for (name, description) in QUERY {
            if ["withTrashed", "onlyTrashed", "withoutTrashed"].contains(name)
                && !resolver
                    .lineage(&typed.model)
                    .unwrap_or_default()
                    .iter()
                    .any(|m| m.soft_deletes)
            {
                continue;
            }
            if !resolver.declared(&typed.model, name) {
                items.insert(
                    format!("{name}()"),
                    method_item(name, format!("{description} · Builder<{short}>"), &ctx),
                );
            }
        }
        for name in [
            "first",
            "firstOrFail",
            "sole",
            "find",
            "findOrFail",
            "findMany",
            "get",
            "count",
            "exists",
            "create",
            "firstOrCreate",
            "updateOrCreate",
            "paginate",
            "simplePaginate",
            "delete",
            "update",
            "pluck",
            "newModelInstance",
        ] {
            if !resolver.declared(&typed.model, name) {
                items.insert(
                    format!("{name}()"),
                    method_item(name, format!("Eloquent query · {short}"), &ctx),
                );
            }
        }
        if typed.shape == Shape::Static {
            for name in ["all", "query"] {
                if !resolver.declared(&typed.model, name) {
                    items.insert(
                        format!("{name}()"),
                        method_item(name, format!("Eloquent model query · {short}"), &ctx),
                    );
                }
            }
        }
        for name in resolver.scopes(&typed.model).keys() {
            let mut item = method_item(name, format!("Local scope · {short}"), &ctx);
            item.sort_text = Some(format!("!{name}"));
            items.insert(format!("{name}()"), item);
        }
        if let Shape::Relation(kind) = &typed.shape {
            let methods: &[&str] = match kind.as_str() {
                "belongsTo" => &["associate", "dissociate"],
                "belongsToMany" | "morphToMany" | "morphedByMany" => &[
                    "attach",
                    "detach",
                    "sync",
                    "syncWithoutDetaching",
                    "toggle",
                    "wherePivot",
                    "withPivot",
                ],
                "hasMany" | "hasOne" | "morphMany" | "morphOne" => {
                    &["save", "saveMany", "createMany"]
                }
                _ => &[],
            };
            for name in methods {
                items.insert(
                    format!("{name}()"),
                    method_item(name, format!("{kind}<{short}>"), &ctx),
                );
            }
        }
    }
    if matches!(typed.shape,Shape::Collection|Shape::MappedCollection(_)) {
        for name in ["each","map","flatMap","filter","reject"]{let mut item=CompletionItem::new(name,CompletionKind::Property);item.detail=Some(format!("Higher-order collection proxy · {short}"));item.replace=Some(Range::new(text::position(&doc.text,ctx.start),text::position(&doc.text,ctx.end)));items.insert(name.into(),item);}
        for name in COLLECTION_KEEP.iter().copied().chain(["map","flatMap",
            "first",
            "last",
            "sole",
            "find",
            "count",
            "isEmpty",
            "isNotEmpty",
            "contains",
            "pluck",
            "map",
            "toArray",
            "modelKeys",
            "load",
        ]) {
            items.insert(
                format!("{name}()"),
                method_item(name, format!("Eloquent collection · {short}"), &ctx),
            );
        }
    }
    if typed.shape==Shape::Model && let Some(object)=resolver.object(&typed.model) {
        for name in object.properties.keys() {let mut item=CompletionItem::new(name,CompletionKind::Property);item.detail=Some(format!("Declared/mixin property · {}",object.full));item.replace=Some(Range::new(text::position(&doc.text,ctx.start),text::position(&doc.text,ctx.end)));items.entry(name.clone()).or_insert(item);}
    }
    if matches!(typed.shape, Shape::Model | Shape::Static) {
        let mut seen = BTreeSet::new();
        for model in resolver.lineage(&typed.model).unwrap_or_default() {
            for name in &model.declared {
                if !seen.insert(name) {
                    continue;
                }
                if model
                    .returns
                    .get(name)
                    .is_some_and(|method| resolver.returned(method, &typed).is_some())
                {
                    if !name.starts_with("__") && !name.starts_with("scope") {
                        items.entry(format!("{name}()")).or_insert_with(|| {
                            method_item(name, format!("Declared method · {}", model.full), &ctx)
                        });
                    }
                }
            }
        }
    }
    items
        .into_values()
        .filter(|item| {
            item.filter_text
                .as_deref()
                .unwrap_or(&item.label)
                .to_ascii_lowercase()
                .starts_with(&ctx.typed.to_ascii_lowercase())
        })
        .take(500)
        .collect()
}
#[cfg(test)]
fn definition(index: &Index, doc: &Document, at: Position) -> Option<Entry> {
    definition_checked(index, doc, at, &|_| true)
}
pub(crate) fn definition_checked(
    index: &Index,
    doc: &Document,
    at: Position,
    owns: &dyn Fn(&str) -> bool,
) -> Option<Entry> {
    let ctx = context(doc, at)?;
    let resolver = Resolver {
        context: &ctx,
        index,
        overlay: model(&doc.text, &doc.path).map(|mut model| {
            let _ = traits::compose(&mut model, &index.eloquent_models, &mut BTreeSet::new(), 0);
            relations::augment(&mut model,&index.eloquent_models);
            model
        }),
        object_overlay:objects::current(&doc.text,&doc.path),
        path: &doc.path,
        owns,
        owner_cache: RefCell::new(BTreeMap::new()),
    };
    let mut typed = receiver(&resolver)?;
    while let Shape::Optional(target)|Shape::Conditional{target,..}=&typed.shape{typed.shape=*target.clone();}
    if matches!(typed.shape,Shape::HigherOrder(..)){typed.shape=Shape::Model;}
    if let Some(full)=resolver.custom_class(&typed)&&let Some(method)=resolver.object_method(&full,ctx.member).filter(|method|!method.static_method&&resolver.owned(&method.entry.path)){return Some(method.entry.clone());}
    if let Shape::Factory{class,..}=&typed.shape{return resolver.raw(class).and_then(|factory|factory.semantic.properties.get("model")).map(|reference|reference.entry.clone());}
    if matches!(typed.shape,Shape::Static|Shape::Model)&&ctx.member=="factory"&&resolver.native_factory_method(&typed.model){return resolver.model_factory(&typed.model).map(|reference|reference.entry);}
    if matches!(typed.shape,Shape::Object|Shape::Facade) || typed.shape == Shape::Static && resolver.get(&typed.model).is_none() {
        let info = resolver.object(&typed.model)?;
        return info.properties.get(ctx.member).filter(|_|typed.shape==Shape::Object).cloned().or_else(|| {
            resolver.object_method(&typed.model,ctx.member).filter(|method|method.static_method==(typed.shape==Shape::Static)).map(|method| method.entry.clone())
        });
    }
    if typed.shape == Shape::Enum {
        let info = resolver.enumeration(&typed.model)?;
        return if ctx.member == "name" || ctx.member == "value" && info.value_type.is_some() {
            Some(info.declaration.clone())
        } else {
            info.methods.get(ctx.member).cloned()
        };
    }
    resolver.get(&typed.model)?;
    if typed.shape==Shape::Model && let Some(property)=resolver.object(&typed.model).and_then(|object|object.properties.get(ctx.member)) {return Some(property.clone());}
    if matches!(typed.shape, Shape::Model | Shape::Static) {
        if let Some(method) = resolver
            .lineage(&typed.model)?
            .into_iter()
            .find_map(|model|model.declared.iter().find(|name|name.eq_ignore_ascii_case(ctx.member)).map(|name|(model,name)))
            .and_then(|(model,name)| model.returns.get(name))
            .filter(|method| method.static_method == (typed.shape == Shape::Static))
        {
            return Some(method.entry.clone());
        }
    }
    match typed.shape {
        Shape::Model => {
            if let Some(entry) = resolver.counts(&typed.model).get(ctx.member) {
                return Some(entry.clone());
            }
            let method = ctx
                .source
                .tokens
                .iter()
                .find(|token| token.start >= ctx.end)
                .is_some_and(|token| token.value == "(");
            if !method&&let Some(accessors)=resolver.accessor_contracts(&typed.model)&&let Some(accessor)=accessors.get(ctx.member){
                let write=ctx.source.tokens.iter().find(|token|token.start>=ctx.end).is_some_and(|token|matches!(token.value,"="|"+="|"-="|"*="|"/="|"++"|"--"|"&="|"|="));
                if write{if let Some(entry)=&accessor.setter_entry{return Some(entry.clone());}}
                else if let Some(entry)=&accessor.getter_entry{return Some(entry.clone());}
                if resolver.table(&typed.model).and_then(|table|index.columns.get(table)?.get(ctx.member)).is_none(){return Some(accessor.entry.clone());}
            }
            if !method && let Some(cast) = resolver.cast(&typed.model, ctx.member) {
                if let Some(info) = cast
                    .target
                    .as_deref()
                    .and_then(|target| resolver.enumeration(target))
                {
                    return Some(info.declaration.clone());
                }
                return resolver
                    .table(&typed.model)
                    .and_then(|table| index.columns.get(table)?.get(ctx.member))
                    .cloned()
                    .or(Some(cast.entry));
            }
            resolver
                .relation(&typed.model, ctx.member)
                .map(|r| r.entry.clone())
                .or_else(|| {
                    resolver
                        .table(&typed.model)
                        .and_then(|table| index.columns.get(table)?.get(ctx.member))
                        .cloned()
                })
        }
        Shape::Static | Shape::Query | Shape::Relation(_) => resolver.package_methods(&typed.model).into_iter().find(|(name,method)|if method.cte{name.eq_ignore_ascii_case(ctx.member)}else{*name==ctx.member}).map(|(_,method)|method.entry.clone()).or_else(||resolver
            .scopes(&typed.model)
            .get(ctx.member)
            .map(|entry| (*entry).clone())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{cursor, doc};
    const ORDER: &str = r#"<?php
namespace App\Models;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Attributes\Scope;
use Domain\Comment as Reply;
class Order extends Model {
 protected $table = 'orders';
 public function customer() { return $this->belongsTo(User::class); }
 public function items() { return $this->hasMany(Reply::class); }
 public function roles() { return $this->belongsToMany(Role::class); }
 public function scopeActive($query) { return $query; }
 #[Scope]
 protected function visible($query) { return $query; }
}
"#;
    fn index() -> Index {
        let mut index = Index::default();
        for (path,text) in [("Order",ORDER.to_owned()),("User",r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; class User extends Model {}"#.into()),("Role",r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; class Role extends Model {}"#.into()),("Comment",r#"<?php namespace Domain; use Illuminate\Database\Eloquent\Model; class Comment extends Model {protected $table='comments';}"#.into())]{cache(&mut index.eloquent_models,&text,&format!("/project/app/Models/{path}.php"));}
        for (table, names) in [
            ("orders", vec!["id", "total"]),
            ("users", vec!["id", "email"]),
            ("comments", vec!["id", "body"]),
            ("roles", vec!["id", "title"]),
        ] {
            for name in names {
                index.columns.entry(table.into()).or_default().insert(
                    name.into(),
                    Entry {
                        path: format!("/project/database/migrations/{table}.php"),
                        at: Position::new(1, 2),
                        width: name.len() as u32,
                        detail: Some("string".into()),
                    },
                );
            }
        }
        index
    }
    fn complete(index: &Index, marked: &str) -> Vec<CompletionItem> {
        let (text, at) = cursor(marked);
        completion(
            index,
            &doc("/project/app/Http/Controllers/Test.php", "php", &text),
            at,
        )
    }
    fn names(index: &Index, marked: &str) -> Vec<String> {
        complete(index, marked)
            .into_iter()
            .map(|i| i.label)
            .collect()
    }
    #[test]
    fn completes_typed_parameter_and_simple_assignment_attributes() {
        let index = index();
        for marked in [
            r#"<?php use App\Models\Order; function show(Order $order) { $order->^ }"#,
            r#"<?php use App\Models\Order; function show(?Order $order) { $order?->^ }"#,
            r#"<?php use App\Models\Order; $order = Order::first(); $order->^"#,
            r#"<?php use App\Models\Order; $order = new Order(); $order->^"#,
            r#"<?php $order = new \App\Models\Order; $order->^"#,
        ] {
            let names = names(&index, marked);
            assert!(names.contains(&"total".into()), "{marked}: {names:?}");
            assert!(names.contains(&"customer".into()));
            assert!(names.contains(&"items()".into()));
            assert!(!names.contains(&"email".into()));
        }
    }
    #[test]
    fn tracks_relation_cardinality_query_and_collection_chains() {
        let index = index();
        for (expression, wanted, absent) in [
            ("Order::query()->active()->first()->", "total", "body"),
            ("Order::first()->customer->", "email", "total"),
            ("Order::first()->items->", "first()", "body"),
            ("Order::first()->items->first()->", "body", "total"),
            (
                "Order::first()->items()->where('id',1)->first()->",
                "body",
                "total",
            ),
            (
                "Order::query()->get()->filter()->first()->",
                "total",
                "body",
            ),
            ("Order::find([1,2])->", "first()", "total"),
            ("Order::first()->roles()->", "attach()", "total"),
            ("Order::first()->customer()->", "associate()", "email"),
        ] {
            let source = format!("<?php use App\\Models\\Order; {expression}^");
            let names = names(&index, &source);
            assert!(names.contains(&wanted.into()), "{source}: {names:?}");
            assert!(!names.contains(&absent.into()), "{source}: {names:?}");
        }
        let scopes = names(&index, r#"<?php use App\Models\Order; Order::^"#);
        assert!(scopes.contains(&"active()".into()));
        assert!(scopes.contains(&"visible()".into()));
    }
    #[test]
    fn resolves_aliases_groups_and_explicit_namespaces_without_short_name_fallback() {
        let index = index();
        for source in [
            r#"<?php use App\Models\Order as Purchase; Purchase::first()->^"#,
            r#"<?php use App\Models\{Order as Purchase, User}; Purchase::first()->^"#,
            r#"<?php namespace App\Models; Order::first()->^"#,
            r#"<?php \App\Models\Order::first()->^"#,
        ] {
            assert!(names(&index, source).contains(&"total".into()), "{source}");
        }
        for source in [
            r#"<?php Order::first()->^"#,
            r#"<?php use Other\Order; Order::first()->^"#,
            r#"<?php \Other\Order::first()->^"#,
            r#"<?php namespace Other; Order::first()->^"#,
        ] {
            assert!(names(&index, source).is_empty(), "{source}");
        }
    }
    #[test]
    fn refuses_unknown_returns_shadowing_and_noncode() {
        let index = index();
        for body in [
            "Order::custom()->^",
            "Order::query()->count()->^",
            "Order::get()->map(fn($x)=>1)->first()->^",
            "$order=Order::first();$order=unknown();$order->^",
            "$order=Order::first();if(true){$order=unknown();}$order->^",
            "$order=Order::first();unset($order);$order->^",
            "$order=Order::first();foreach([] as $order){$order->^}",
            "$order=Order::first();fn($order)=>$order->^",
            "$order=Order::first();function nested(){ $order->^ }",
            "'$order->^'",
            "// $order->^",
            "/* Order::first()->^ */",
            "$order=Order::first(); [$order]=[];$order->^",
            "Order::paginate()->^",
        ] {
            let source = format!("<?php use App\\Models\\Order; {body}");
            assert!(names(&index, &source).is_empty(), "{source}");
        }
    }
    #[test]
    fn replacement_is_utf16_prefix_filtered_and_does_not_duplicate_parentheses() {
        let index = index();
        let marked = "<?php use App\\Models\\Order; /* café 🦀 */ Order::first()->to^tal;";
        let (source, at) = cursor(marked);
        let items = completion(&index, &doc("/project/x.php", "php", &source), at);
        let total = items.iter().find(|i| i.label == "total").unwrap();
        let range = total.replace.unwrap();
        assert_eq!(
            &source[text::offset(&source, range.start).unwrap()
                ..text::offset(&source, range.end).unwrap()],
            "total"
        );
        let items = complete(
            &index,
            r#"<?php use App\Models\Order; Order::wh^ere('id',1);"#,
        );
        let item = items.iter().find(|i| i.label == "where()").unwrap();
        assert_eq!(item.insert_text, "where");
        assert!(!item.snippet);
    }
    #[test]
    fn this_uses_current_model_text_and_declines_static_methods() {
        let index = index();
        let marked = ORDER.replace(
            "public function customer()",
            "public function demo(){ $this->^ } public function customer()",
        );
        let (source, at) = cursor(&marked);
        let document = doc("/project/app/Models/Order.php", "php", &source);
        assert!(
            completion(&index, &document, at)
                .iter()
                .any(|i| i.label == "total")
        );
        let marked = marked.replace("public function demo()", "public static function demo()");
        let (source, at) = cursor(&marked);
        assert!(completion(&index, &doc(&document.path, "php", &source), at).is_empty());
        let marked = marked.replace("extends Model", "extends Unknown");
        let (source, at) = cursor(&marked);
        assert!(completion(&index, &doc(&document.path, "php", &source), at).is_empty());
    }
    #[test]
    fn definition_targets_relation_declaration_and_attribute_migration() {
        let index = index();
        for (member, path) in [
            ("customer", "/project/app/Models/Order.php"),
            ("total", "/project/database/migrations/orders.php"),
        ] {
            let source = format!("<?php use App\\Models\\Order; Order::first()->{member}^");
            let (source, at) = cursor(&source);
            let result = definition(&index, &doc("/project/x.php", "php", &source), at).unwrap();
            assert_eq!(result.path, path);
        }
    }
    #[test]
    fn nonmodels_and_ambiguous_model_declarations_are_not_inferred() {
        let mut index = index();
        cache(
            &mut index.eloquent_models,
            r#"<?php namespace Other; class Order {}"#,
            "/project/app/Models/Dto.php",
        );
        assert!(names(&index, r#"<?php use Other\Order; Order::first()->^"#).is_empty());
        cache(
            &mut index.eloquent_models,
            ORDER,
            "/project/app/Models/Copy.php",
        );
        assert!(names(&index, r#"<?php use App\Models\Order; Order::first()->^"#).is_empty());
    }
}

#[cfg(test)]
mod inheritance_tests {
    use super::*;
    use crate::tests::{cursor, doc};
    #[test]
    fn inherits_known_relations_scopes_and_explicit_parent_table_but_honors_overrides() {
        let mut index = Index::default();
        for (path, source) in [
            (
                "Base",
                r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; abstract class Base extends Model { protected $table='records'; public function parent(){return $this->belongsTo(Child::class);} public function scopeActive($q){return $q;} }"#,
            ),
            (
                "Child",
                r#"<?php namespace App\Models; class Child extends Base {}"#,
            ),
            (
                "Override",
                r#"<?php namespace App\Models; class Override extends Base {public function parent(){return 1;}}"#,
            ),
        ] {
            cache(
                &mut index.eloquent_models,
                source,
                &format!("/project/app/Models/{path}.php"),
            );
        }
        index.columns.entry("records".into()).or_default().insert(
            "title".into(),
            Entry {
                path: "/project/database/migrations/create.php".into(),
                at: Position::new(0, 0),
                width: 5,
                detail: None,
            },
        );
        let lookup = |source: &str| {
            let (text, at) = cursor(source);
            completion(&index, &doc("/project/x.php", "php", &text), at)
                .into_iter()
                .map(|i| i.label)
                .collect::<Vec<_>>()
        };
        let labels = lookup(r#"<?php use App\Models\Child; Child::first()->^"#);
        assert!(labels.contains(&"title".into()));
        assert!(labels.contains(&"parent".into()));
        let labels = lookup(r#"<?php use App\Models\Child; Child::active()->first()->parent->^"#);
        assert!(labels.contains(&"title".into()));
        let labels = lookup(r#"<?php use App\Models\Override; Override::first()->^"#);
        assert!(!labels.contains(&"parent".into()));
        assert!(!labels.contains(&"parent()".into()));
    }
    #[test]
    fn unresolved_or_cyclic_parent_classes_do_not_become_models() {
        let mut index = Index::default();
        for (name, parent) in [("A", "B"), ("B", "A"), ("C", "Unknown")] {
            cache(
                &mut index.eloquent_models,
                &format!("<?php namespace App\\Models; class {name} extends {parent} {{}}"),
                &format!("/project/app/Models/{name}.php"),
            );
        }
        for name in ["A", "B", "C"] {
            let (text, at) = cursor(&format!("<?php use App\\Models\\{name}; {name}::^"));
            assert!(completion(&index, &doc("/project/x.php", "php", &text), at).is_empty());
        }
    }
}

#[cfg(test)]
mod cast_tests {
    use super::*;
    use crate::tests::{cursor, doc};
    fn setup() -> Index {
        let mut index = Index::default();
        cache(
            &mut index.eloquent_models,
            r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; use App\Enums\Status as State; class Purchase extends Model { protected $casts=['status'=>State::class,'quantity'=>'integer']; }"#,
            "/project/app/Models/Purchase.php",
        );
        cache_enum(
            &mut index.eloquent_enums,
            r#"<?php namespace App\Enums; enum Status:string{case Paid='paid'; public function label():string{return 'Paid';}private function secret(){}public static function defaultCase(){} }"#,
            "/project/app/Enums/Status.php",
        );
        index.columns.entry("purchases".into()).or_default().insert(
            "status".into(),
            Entry {
                path: "/project/database/migrations/table.php".into(),
                at: Position::new(1, 0),
                width: 6,
                detail: Some("string".into()),
            },
        );
        index
    }
    #[test]
    fn enum_cast_properties_and_public_methods_are_completed_and_navigated() {
        let index = setup();
        let (text, at) = cursor(r#"<?php use App\Models\Purchase; Purchase::first()->status->^"#);
        let document = doc("/project/x.php", "php", &text);
        let names: Vec<_> = completion(&index, &document, at)
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert_eq!(names, ["name", "value", "label()"]);
        let (text, at) =
            cursor(r#"<?php use App\Models\Purchase; Purchase::first()->status->la^bel();"#);
        assert_eq!(
            definition(&index, &doc(&document.path, "php", &text), at)
                .unwrap()
                .path,
            "/project/app/Enums/Status.php"
        );
        let (text, at) = cursor(
            r#"<?php use App\Models\Purchase; $status=Purchase::first()->status;$status->va^"#,
        );
        assert_eq!(
            completion(&index, &doc(&document.path, "php", &text), at)[0].label,
            "value"
        );
        assert!(
            completion_checked(
                &index,
                &document,
                crate::tests::cursor(
                    r#"<?php use App\Models\Purchase; Purchase::first()->status->^"#
                )
                .1,
                &|path| !path.contains("Enums")
            )
            .is_empty()
        );
    }
    #[test]
    fn casts_method_and_unit_enums_do_not_invent_backed_values() {
        let mut index = setup();
        cache(
            &mut index.eloquent_models,
            r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; use App\Enums\Choice; class Record extends Model {protected function casts():array{return ['status'=>Choice::class];}}"#,
            "/project/app/Models/Record.php",
        );
        cache_enum(
            &mut index.eloquent_enums,
            r#"<?php namespace App\Enums; enum Choice {case Yes;}"#,
            "/project/app/Enums/Choice.php",
        );
        let (text, at) = cursor(r#"<?php use App\Models\Record; Record::first()->status->^"#);
        let names: Vec<_> = completion(&index, &doc("/project/x.php", "php", &text), at)
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert_eq!(names, ["name"]);
    }
    #[test]
    fn unknown_custom_casts_do_not_become_enum_objects() {
        let mut index = setup();
        cache(
            &mut index.eloquent_models,
            r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; class Custom extends Model {protected $casts=['status'=>UnknownCaster::class];}"#,
            "/project/app/Models/Custom.php",
        );
        let (text, at) = cursor(r#"<?php use App\Models\Custom; Custom::first()->status->^"#);
        assert!(completion(&index, &doc("/project/x.php", "php", &text), at).is_empty());
    }
    #[test]
    fn relations_returning_models_instead_of_relation_objects_are_not_published() {
        let mut index = setup();
        cache(
            &mut index.eloquent_models,
            r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; class InvalidRelation extends Model {public function purchase(){return $this->hasMany(Purchase::class)->first();}}"#,
            "/project/app/Models/InvalidRelation.php",
        );
        let (text, at) =
            cursor(r#"<?php use App\Models\InvalidRelation; InvalidRelation::first()->^"#);
        let names: Vec<_> = completion(&index, &doc("/project/x.php", "php", &text), at)
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert!(!names.contains(&"purchase".into()));
    }
}

#[cfg(test)]
mod bounded_find_tests {
    use super::*;
    use crate::tests::{cursor, doc};
    #[test]
    fn find_overloads_respect_scalar_and_array_ids_without_guessing_untyped_variables() {
        let mut index = Index::default();
        cache(
            &mut index.eloquent_models,
            r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; class Order extends Model {}"#,
            "/project/app/Models/Order.php",
        );
        for (expression, wanted) in [
            ("function f(int $id){Order::find($id)->^}", Some("save()")),
            (
                "function f(array $ids){Order::find($ids)->^}",
                Some("first()"),
            ),
            ("function f($id){Order::find($id)->^}", None),
            ("Order::find(array(1,2))->^", None),
            ("function f(array|int $id){Order::find($id)->^}", None),
            (
                "function f(int $id){$id += unknown();Order::find($id)->^}",
                None,
            ),
            ("Order::find(unknown())->^", None),
        ] {
            let (text, at) = cursor(&format!("<?php use App\\Models\\Order; {expression}"));
            let items = completion(&index, &doc("/project/x.php", "php", &text), at);
            if let Some(wanted) = wanted {
                assert!(items.iter().any(|i| i.label == wanted), "{expression}");
            } else {
                assert!(items.is_empty(), "{expression}");
            }
        }
    }
}

#[cfg(test)]
mod inference_boundary_tests {
    use super::*;
    use crate::tests::{cursor, doc};
    #[test]
    fn fully_qualified_cast_targets_keep_their_root_namespace() {
        let mut index = Index::default();
        cache(
            &mut index.eloquent_models,
            r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; class Record extends Model { protected $casts=['status'=>\App\Enums\Status::class]; }"#,
            "/project/app/Models/Record.php",
        );
        cache_enum(
            &mut index.eloquent_enums,
            r#"<?php namespace App\Enums; enum Status:string {case Paid='paid';}"#,
            "/project/app/Enums/Status.php",
        );
        let (text, at) = cursor(r#"<?php use App\Models\Record; Record::first()->status->va^"#);
        assert_eq!(
            completion(&index, &doc("/project/x.php", "php", &text), at)[0].label,
            "value"
        );
    }
    #[test]
    fn custom_find_and_underlying_database_query_do_not_infer_models() {
        let mut index = Index::default();
        cache(
            &mut index.eloquent_models,
            r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; class Record extends Model { public static function find($id){return 1;} }"#,
            "/project/app/Models/Record.php",
        );
        for expression in [
            "Record::find(1)->^",
            "Record::query()->getQuery()->first()->^",
        ] {
            let (text, at) = cursor(&format!(r"<?php use App\Models\Record; {expression}"));
            assert!(completion(&index, &doc("/project/x.php", "php", &text), at).is_empty());
        }
    }
    #[test]
    fn soft_delete_query_methods_require_the_known_trait() {
        let mut index = Index::default();
        for (name, body) in [
            ("Plain", ""),
            ("Deleted", r"use \Illuminate\Database\Eloquent\SoftDeletes;"),
        ] {
            cache(
                &mut index.eloquent_models,
                &format!(
                    r"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; class {name} extends Model {{{body}}}"
                ),
                &format!("/project/app/Models/{name}.php"),
            );
            let (text, at) = cursor(&format!("<?php use App\\Models\\{name}; {name}::withTra^"));
            assert_eq!(
                !completion(&index, &doc("/project/x.php", "php", &text), at).is_empty(),
                name == "Deleted"
            );
        }
    }
}

#[cfg(test)]
mod dynamic_declaration_tests {
    use super::*;
    #[test]
    fn dynamic_table_and_cast_expressions_are_withheld() {
        let model=model(r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; class Record extends Model { protected $table='records'.suffix(); protected $casts=['status'=>State::class]+other(); protected function casts():array{return ['status'=>State::class]+other();} }"#, "/project/app/Models/Record.php").unwrap();
        assert!(model.table.is_none());
        assert!(matches!(model.property_casts, Some(None)));
        assert!(matches!(model.method_casts, Some(None)));
    }
}

/// Shared PHP namespace/import resolution for literal class-name constants.
/// Resolves names only; it does not require or execute the target class.
pub(crate) fn source_class_name(text: &str, name: &str) -> Option<String> {
    if ["self", "static", "parent"]
        .iter()
        .any(|n| name.eq_ignore_ascii_case(n))
        || name.is_empty()
        || !name
            .bytes()
            .all(|b| b>=128 || b.is_ascii_alphanumeric() || matches!(b, b'_' | b'\\'))
    {
        return None;
    }
    // Route files resolve one class per route against the same text: parsing the
    // imports once per text keeps a 120-route file from taking 30 ms per call.
    thread_local! { static LAST: std::cell::RefCell<Option<(String, Option<Names>)>> = const { std::cell::RefCell::new(None) }; }
    LAST.with(|last| {
        let mut last = last.borrow_mut();
        if last.as_ref().is_none_or(|(cached, _)| cached != text) {
            let names = Source::new(text).and_then(|source| Names::parse(&source));
            *last = Some((text.to_owned(), names));
        }
        last.as_ref()?.1.as_ref().map(|names| names.resolve(name))
    })
}

/// Constructor-forwarded signatures use cached owned declarations, never source reads.
pub(crate) fn forwarded_signature_help_checked(index:&Index,doc:&Document,at:Position,owns:&dyn Fn(&str)->bool)->Option<openidea_plugin::language_features::SignatureHelp>{
    use openidea_plugin::language_features::{SignatureHelp,Signature,Parameter,ParameterLabel};
    let offset=text::offset(&doc.text,at)?;let source=Source::new(&doc.text)?;let names=Names::parse(&source)?;
    let open=objects::call_open(&source,offset)?;
    if open<3||!source.is(open-2,"::")||source.tokens[open-3].kind!=Kind::Name||source.tokens[open-1].kind!=Kind::Name{return None;}
    let full=names.resolve(source.tokens[open-3].value);let method=source.tokens[open-1].value;
    let signature=index.metadata.forwarded.get(&(full.to_ascii_lowercase(),method.to_ascii_lowercase()))?;
    if !owns(&signature.entry.path)||!signature.sources.iter().all(|path|owns(path)){return None;}
    let parameter=objects::active_parameter(&source,open,offset,&signature.parameters)?;
    Some(SignatureHelp{signatures:vec![Signature{label:format!("{full}::{method}({}): {}",signature.parameters.join(", "),signature.target),documentation:Some("Declared constructor parameters forwarded through project metadata.".into()),parameters:signature.parameters.iter().map(|p|Parameter{label:ParameterLabel::Text(p.clone()),documentation:None}).collect()}],active_signature:0,active_parameter:parameter})
}

#[cfg(test)]
mod advanced_tests {
    use super::*;
    use crate::tests::{cursor, doc};
    fn inventory() -> Index {
        let mut index = Index::default();
        for (path, source) in [
            (
                "app/Models/Order.php",
                r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; use App\Concerns\HasOwner; use App\Casts\AddressCast; class Order extends Model { use HasOwner; protected $casts=['address'=>AddressCast::class]; public function customer(): ?User {return new User;} public static function sample(): static {return new static;} protected function hidden(): User {return new User;} }"#,
            ),
            (
                "app/Models/User.php",
                r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; class User extends Model {}"#,
            ),
            (
                "app/Concerns/HasOwner.php",
                r#"<?php namespace App\Concerns; use App\Models\User; use App\Traits\Active; trait HasOwner {use Active; public function owner(){return $this->belongsTo(User::class);} }"#,
            ),
            (
                "app/Traits/Active.php",
                r#"<?php namespace App\Traits; trait Active {public function scopeActive($query){return $query;}}"#,
            ),
        ] {
            cache(
                &mut index.eloquent_models,
                source,
                &format!("/project/{path}"),
            );
        }
        for (path, source) in [
            (
                "app/Casts/AddressCast.php",
                r#"<?php namespace App\Casts; use Illuminate\Contracts\Database\Eloquent\CastsAttributes; use App\ValueObjects\Address; class AddressCast implements CastsAttributes { public function get($model,$key,$value,$attributes): Address {return new Address;} }"#,
            ),
            (
                "app/ValueObjects/Address.php",
                r#"<?php namespace App\ValueObjects; class Address {public string $lineOne; private string $secret; public function __construct(public string $city, protected string $internal){} public function normalized(): self {return $this;} public function format(): string {return '';} private function hidden(): self {return $this;} }"#,
            ),
        ] {
            cache_object(
                &mut index.eloquent_objects,
                source,
                &format!("/project/{path}"),
            );
        }
        for (table, names) in [
            ("orders", vec!["id", "total"]),
            ("users", vec!["id", "email"]),
        ] {
            for name in names {
                index.columns.entry(table.into()).or_default().insert(
                    name.into(),
                    Entry {
                        path: format!("/project/database/migrations/{table}.php"),
                        at: Position::new(0, 0),
                        width: name.len() as u32,
                        detail: None,
                    },
                );
            }
        }
        index
    }
    fn labels(index: &Index, source: &str) -> Vec<String> {
        let (text, at) = cursor(source);
        completion(index, &doc("/project/Test.php", "php", &text), at)
            .into_iter()
            .map(|i| i.label)
            .collect()
    }
    #[test]
    fn composes_nested_trait_relations_scopes_and_navigation_without_php() {
        let index = inventory();
        assert!(
            labels(
                &index,
                r#"<?php use App\Models\Order; Order::active()->first()->owner->^"#
            )
            .contains(&"email".into())
        );
        let (text, at) = cursor(r#"<?php use App\Models\Order; Order::first()->ow^ner"#);
        let target = definition(&index, &doc("/project/Test.php", "php", &text), at).unwrap();
        assert_eq!(target.path, "/project/app/Concerns/HasOwner.php");
        let names = completion_checked(
            &index,
            &doc("/project/Test.php", "php", &text),
            at,
            &|path| !path.contains("Concerns"),
        );
        assert!(!names.iter().any(|i| i.label == "owner"));
    }
    #[test]
    fn custom_casts_and_typed_method_chains_expose_only_public_known_members() {
        let index = inventory();
        for source in [
            r#"<?php use App\Models\Order; Order::first()->address->^"#,
            r#"<?php use App\Models\Order; Order::first()->address->normalized()->^"#,
        ] {
            let names = labels(&index, source);
            for member in ["lineOne", "city", "normalized()", "format()"] {
                assert!(names.contains(&member.into()), "{names:?}");
            }
            for member in ["secret", "internal", "hidden()"] {
                assert!(!names.contains(&member.into()));
            }
        }
        for source in [
            r#"<?php use App\Models\Order; Order::first()->customer()?->^"#,
            r#"<?php use App\Models\Order; Order::sample()->customer()->^"#,
        ] {
            assert!(labels(&index, source).contains(&"email".into()));
        }
        assert!(
            labels(
                &index,
                r#"<?php use App\Models\Order; Order::query()->customer()->^"#
            )
            .is_empty()
        );
        assert!(
            labels(
                &index,
                r#"<?php use App\Models\Order; Order::first()->hidden()->^"#
            )
            .is_empty()
        );
        assert!(
            labels(
                &index,
                r#"<?php use App\Models\Order; Order::first()->address->format()->^"#
            )
            .is_empty()
        );
        let (text, at) = cursor(r#"<?php use App\Models\Order; Order::first()->address->ci^ty"#);
        assert_eq!(
            definition(&index, &doc("/project/Test.php", "php", &text), at)
                .unwrap()
                .path,
            "/project/app/ValueObjects/Address.php"
        );
    }
    #[test]
    fn custom_cast_literal_names_and_parameters_keep_the_explicit_getter_type() {
        for cast in [
            r"\App\Casts\AddressCast::class . ':parameter'",
            r"'App\Casts\AddressCast:parameter'",
        ] {
            let mut index = inventory();
            index.eloquent_models.remove("app\\models\\order");
            cache(
                &mut index.eloquent_models,
                &format!(
                    "<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Model; class Order extends Model {{protected $casts=['address'=>{cast}];}}"
                ),
                "/project/app/Models/Order.php",
            );
            assert!(
                labels(
                    &index,
                    r#"<?php use App\Models\Order; Order::first()->address->^"#
                )
                .contains(&"city".into()),
                "{cast}"
            );
        }
    }
    #[test]
    fn trait_conflicts_adaptations_and_cycles_do_not_invent_members() {
        for source in [
            r#"use \App\Concerns\HasOwner {owner as renamed;}"#,
            r#"use \App\Concerns\HasOwner, \App\Traits\Other;"#,
        ] {
            let mut index = inventory();
            index.eloquent_models.remove("app\\models\\order");
            cache(
                &mut index.eloquent_models,
                &format!(
                    "<?php namespace App\\Models; use Illuminate\\Database\\Eloquent\\Model; class Order extends Model {{{source}}}"
                ),
                "/project/app/Models/Order.php",
            );
            cache(
                &mut index.eloquent_models,
                r#"<?php namespace App\Traits; use App\Models\User; trait Other {public function owner(){return $this->belongsTo(User::class);}}"#,
                "/project/app/Traits/Other.php",
            );
            assert!(
                labels(
                    &index,
                    r#"<?php use App\Models\Order; Order::first()->owner->^"#
                )
                .is_empty()
            );
            assert!(
                labels(&index, r#"<?php use App\Models\Order; Order::first()->^"#)
                    .contains(&"total".into())
            );
        }
        let mut index = inventory();
        index.eloquent_models.remove("app\\traits\\active");
        cache(
            &mut index.eloquent_models,
            r#"<?php namespace App\Traits; use App\Concerns\HasOwner; trait Active {use HasOwner;}"#,
            "/project/app/Traits/Active.php",
        );
        assert!(
            labels(
                &index,
                r#"<?php use App\Models\Order; Order::first()->owner->^"#
            )
            .is_empty()
        );
    }
}

#[cfg(test)]
#[path = "eloquent_deeper_tests.rs"]
mod deeper_tests;

#[cfg(test)]mod method_token_tests{
    use super::*;
    #[test]fn package_relation_adapter_preserves_inherited_method_and_relation_precedence(){
        let mut models=Models::new();
        for(name,parent,methods)in [("Base","\\Illuminate\\Database\\Eloquent\\Model","public function posts(){return null;}"),("Child","Base", ""),("Other","Base",""),("Clean","\\Illuminate\\Database\\Eloquent\\Model","")]{cache(&mut models,&format!("<?php namespace App;class {name} extends {parent}{{{methods}}}"),&format!("/project/{name}.php"));}
        let fact=|owner:&str,name:&str|DeclaredRelation{owner:format!("App\\{owner}"),owner_path:format!("/project/{owner}.php"),name:name.into(),target:"App\\Clean".into(),kind:"hasMany".into(),many:true,entry:Entry{path:format!("/project/{owner}.php"),at:Position::new(0,0),width:5,detail:None},evidence_paths:vec![]};
        apply_declared_relations(&mut models,&[fact("Child","posts"),fact("Other","POSTS"),fact("Clean","posts")]);
        assert!(!models["app\\child"].relations.contains_key("posts"));assert!(!models["app\\other"].relations.contains_key("POSTS"));assert!(models["app\\clean"].relations.contains_key("posts"));
    }
    #[test]fn declared_graph_inventory_includes_abstract_private_methods_and_rejects_adaptations(){
        let text="<?php namespace App;use Vendor\\Concern as C;abstract class Base extends ParentBase{use C;abstract protected function required();PRIVATE function hidden(){} public function shown(){function nested(){}}}";
        let(full,parent,traits,methods)=declared_method_inventory(text,"/project/Base.php").unwrap();assert_eq!(full,"App\\Base");assert_eq!(parent,"App\\ParentBase");assert_eq!(traits,vec!["Vendor\\Concern"]);assert_eq!(methods.into_iter().map(|(name,_)|name).collect::<Vec<_>>(),vec!["required","hidden","shown"]);
        assert!(declared_method_inventory("<?php class A{use T{method as alias;}}","A.php").is_none());
    }
    #[test]fn integer_and_public_constant_literals_follow_php_radices_and_visibility(){
        for(raw,value)in [("010",8),("0o10",8),("0xFF",255),("0b1_010",10),("1_000",1000),("-010",-8),("-9223372036854775807",-i64::MAX)]{assert_eq!(integer_literal(raw),Some(value),"{raw}");}
        for raw in ["008","1__0","0x_FF","9223372036854775808","-9223372036854775808","1+2","1.0"]{assert_eq!(integer_literal(raw),None,"{raw}");}
        let(full,values)=literal_constants("<?php namespace App;class Keys{const ENABLED='feature.enabled', FLAG=010;private const SECRET='hidden';public const string NAME=\"feature.name\";const DYNAMIC=make();}").unwrap();assert_eq!(full,"App\\Keys");assert_eq!(values["ENABLED"],"feature.enabled");assert_eq!(values["FLAG"],8);assert_eq!(values["NAME"],"feature.name");assert!(!values.contains_key("SECRET"));assert!(!values.contains_key("DYNAMIC"));
    }
    #[test]fn own_string_property_requires_direct_unambiguous_literal_declaration(){
        assert_eq!(class_string_property(r#"<?php class Factory{protected static $namespace='Database\\Factories\\';public function f($namespace='Fake'){}}"#,"namespace").as_deref(),Some("Database\\Factories\\"));
        for source in ["<?php class A{protected $namespace=PREFIX;}","<?php class A{protected $namespace='x'.suffix();}","<?php class A{function f($namespace='x'){}}","<?php class A{protected $namespace='x';protected $namespace='y';}"]{assert!(class_string_property(source,"namespace").is_none(),"{source}");}
    }
    #[test]fn own_method_profile_retains_signature_body_and_string_literals(){
        let source=r#"<?php namespace Example;class Factory{protected $value='decoy function modelName(){}';/** docs */#[Marker('x')]protected static function modelName(string $model='Post'):string{/* ignored */return $model.'\\Model';}public function other(){function modelName(){}}}"#;
        let tokens=class_method_tokens(source,"MODELNAME").unwrap();assert_eq!(&tokens[..8],&["#","[","Marker","(","'x'",")","]","protected"]);assert!(tokens.windows(3).any(|tokens|tokens==["static","function","modelName"]));assert!(tokens.iter().any(|token|token=="'Post'"));assert_eq!(tokens.last().unwrap(),"}");assert!(!tokens.iter().any(|token|token.contains("ignored")));
        assert!(class_method_tokens("<?php class A{function modelName(){}function MODELNAME(){}}","modelName").is_none());
        assert!(class_method_tokens("<?php class A{function modelName(){","modelName").is_none());
        assert!(class_method_tokens("<?php trait A{function modelName(){}}","modelName").is_none());
    }
}
