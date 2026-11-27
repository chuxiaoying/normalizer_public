//! Source-level entry identity and bindings for the business PERS document.
use super::{identifier, Loaded, ExecutionResult, Result};
use math::{Expr, SymVarOrigin};
use serde::{Deserialize, Serialize};
use serde_json::json;
use simulator_parser::{JavaMethod, JavaType};
use std::{collections::BTreeMap, path::Path};

pub(super) struct BusinessObservations {
    pub(super) returned: Option<String>,
    pub(super) tables: BTreeMap<String, BTreeMap<String, String>>,
    pub(super) effects: Vec<serde_json::Value>,
    pub(super) acquisitions: BTreeMap<String, serde_json::Value>,
    pub(super) throws: Vec<serde_json::Value>,
}

enum BusinessOutput {
    Pure(Expr),
    Observed(BusinessObservations),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EntryContext {
    function: String,
    #[serde(default)]
    external_results: BTreeMap<String, String>,
    returns: String,
    parameters: BTreeMap<String, Parameter>,
    #[serde(default)]
    types: BTreeMap<String, Record>,
    #[serde(default)]
    environment: BTreeMap<String, State>,
    #[serde(default)]
    bindings: BTreeMap<String, StateBinding>,
}

struct StateBinding {
    root: String,
    fields: Vec<String>,
}

impl<'de> Deserialize<'de> for StateBinding {
    fn deserialize<D: serde::Deserializer<'de>>(reader: D) -> std::result::Result<Self, D::Error> {
        let parts = Vec::<String>::deserialize(reader)?;
        let mut parts = parts.into_iter();
        let kind = parts.next();
        let root = parts.next();
        let fields: Vec<_> = parts.collect();
        match (kind.as_deref(), root) {
            (Some("E"), Some(root)) if !fields.is_empty() => Ok(Self { root, fields }),
            _ => Err(serde::de::Error::custom("invalid business source binding")),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Parameter {
    position: usize,
    #[serde(rename = "type")]
    ty: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct State {
    kind: String,
    source: String,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    ty: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    fields: BTreeMap<String, Field>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    fields: BTreeMap<String, String>,
}

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
enum Field {
    Leaf(String),
    Record(BTreeMap<String, Field>),
}

fn source_type(ty: &JavaType) -> Result<String> {
    match ty {
        JavaType::Void => Ok("void".into()),
        t if crate::normalized::exact_decimal_type(t) || crate::normalized::native_decimal_type(t) => Ok("number".into()),
        t if crate::normalized::calendar_date_type(t) => Ok("date".into()),
        JavaType::Array(item) => Ok(format!("{}[]", source_type(item)?)),
        JavaType::Primitive(t) if matches!(t.as_str(), "byte" | "short" | "int" | "long") => Ok("number".into()),
        JavaType::Primitive(t) if t == "boolean" => Ok("bool".into()),
        JavaType::Class(t) if matches!(t.as_str(), "String" | "java.lang.String") => Ok("text".into()),
        JavaType::Class(t) if identifier(t) => Ok(t.clone()),
        _ => Err(format!("[NORM-BUSINESS-TYPE] unsupported source type {ty:?}")),
    }
}

impl Loaded {
    fn entry_method(&self, entry: &str) -> Result<(&simulator_parser::JavaClass, &JavaMethod)> {
        let (owner, name) = entry.rsplit_once('.').ok_or("expected Class.method")?;
        let class = self.program.find_class(owner).ok_or("entry class not found")?;
        let method = if self.project.direct_source {
            crate::normalized::method_for_key(class, entry)
        } else {
            class.methods().iter().find(|m| m.name() == name)
        }.ok_or("entry method not found")?;
        Ok((class, method))
    }

    pub fn source_pers(&self, root: &Path, entry: &str) -> Result<String> {
        let selected = self.source_entry.as_ref();
        let method_key = match selected {
            Some(source) if source.locator == entry => source.target.as_str(),
            Some(_) => return Err("[NORM-ENTRY] selected source locator does not match".into()),
            None => entry,
        };
        let (class, method) = self.entry_method(method_key)?;
        let source = Path::new(class.source_file().ok_or("missing entry source location")?);
        let path = source.strip_prefix(root).map_err(|_| "entry file outside project")?;
        let result = self.normalize(method_key)?;
        let signature = method.params().iter().map(|(ty, _)| ty).chain([method.return_type()]);
        let mut types = BTreeMap::new();
        let mut environment = BTreeMap::new();
        let mut pending = signature.collect::<Vec<_>>();
        let mut external_results = BTreeMap::new();
        for event in result.realtime_effects.events() {
            if let super::realtime_state::EffectOperation::Port { id, .. } = &event.operation {
                let port = self.project.source_ports.get(id).ok_or("missing source port declaration")?;
                external_results.insert(id.clone(), source_type(&port.returns)?.to_owned());
                pending.push(&port.returns);
            }
        }
        for name in result.source_state_bindings.values() {
            let field = class.fields().iter().find(|f| f.name() == name && crate::normalized::entry_state(class, f))
                .ok_or("unresolved source state declaration")?;
            let ty = source_type(field.java_type())?.to_owned();
            pending.push(field.java_type());
            let identity = selected.and_then(|source| source.receiver.as_ref().map(|receiver| {
                let file = source.locator.split_once('#').expect("validated entry locator").0;
                format!("{file}#{}.{receiver_field}.{name}", receiver.owner, receiver_field = receiver.field)
            })).unwrap_or_else(|| format!("{}#{}.{name}", path.display(), class.full_name()));
            environment.insert(name.clone(), State {
                kind: "state".into(), source: identity,
                ty: Some(ty), fields: BTreeMap::new(),
            });
        }
        while let Some(ty) = pending.pop() {
            if let JavaType::Array(item) = ty { pending.push(item); continue; }
            let JavaType::Class(name) = ty else { continue };
            if source_type(ty)? == "text" || crate::normalized::exact_decimal_type(ty) || crate::normalized::calendar_date_type(ty) || types.contains_key(name) { continue; }
            let declaration = self.program.find_class(name).ok_or("missing named business type")?;
            let mut fields = BTreeMap::new();
            for field in declaration.fields().iter().filter(|f| !f.is_static()) {
                let mut domain = source_type(field.java_type())?.to_owned();
                if crate::normalized::source_nullable_field(declaration, field) {
                    domain.push('?');
                }
                fields.insert(field.name().to_owned(), domain);
                pending.push(field.java_type());
            }
            types.insert(name.clone(), Record { fields });
        }
        let context = EntryContext {
            external_results,
            function: selected.map(|source| source.locator.clone())
                .unwrap_or_else(|| format!("{}#{entry}", path.display())),
            types,
            returns: {
                let mut domain = source_type(method.return_type())?;
                if crate::normalized::nullable_text_return(class, method) { domain.push('?'); }
                domain
            },
            parameters: method.params().iter().enumerate().map(|(position, (ty, name))| {
                let mut domain = source_type(ty)?.to_owned();
                if self.project.source_nullable_entry_parameters.contains(name) {
                    domain.push('?');
                }
                Ok((name.clone(), Parameter { position, ty: domain }))
            }).collect::<Result<_>>()?,
            environment,
            bindings: BTreeMap::new(),
        };
        context.render_source(&self.project, &result, method)
    }

    /// Validate and run one closed adapter entry; do not execute unrelated methods.
    pub fn project_pers(path: &Path, entry: &str) -> Result<String> {
        let loaded = Self::read_adapter_project(path)?;
        let context = loaded.project.entry_contexts.get(entry);
        if let Some(context) = context {
            context.validate_public_bindings()?;
        }
        let result = loaded.normalize(entry)?;
        context.ok_or("missing business entry context")?
            .render_source(&loaded.project, &result, loaded.entry_method(entry)?.1)
    }
}

impl EntryContext {
    // Native source constructs this identity and dense parameter positions
    // from its selected AST. Only the adapter accepts supplied values.
    fn validate_public_bindings(&self) -> Result<()> {
        if !self.function.contains('#') { return Err("missing business entry identity".into()); }
        let mut positions: Vec<_> = self.parameters.values().map(|p| p.position).collect();
        positions.sort_unstable();
        if !positions.into_iter().eq(0..self.parameters.len()) {
            return Err("invalid business parameter positions".into());
        }
        Ok(())
    }

    pub(super) fn return_type(&self) -> &str { &self.returns }

    // The direct-source executor must already have certified that this
    // port uses the selected instance's bound receiver.
    pub(super) fn direct_port_identity(&self, endpoint: &str) -> Result<String> {
        let (file, invocation) = self.function.split_once('#').ok_or("missing entry source identity")?;
        let locator = invocation.split_once('(').map_or(invocation, |(prefix, _)| prefix);
        let (receiver, _) = locator.rsplit_once('.').ok_or("missing selected instance locator")?;
        Ok(format!("{file}#{receiver}.{endpoint}"))
    }


    pub(super) fn parameter_type(&self, name: &str) -> Option<&str> {
        self.parameters.get(name).map(|p| p.ty.as_str())
    }

    pub(super) fn state_field(&self, root: &str, fields: &[String]) -> Result<&str> {
        if fields.is_empty() { return Err("state path requires a field".into()); }
        let leaf = self.state_shape_at(root, fields)?.ok_or("expected a scalar state field")?;
        if self.types.contains_key(leaf.trim_end_matches('?')) {
            return Err("expected a scalar state field".into());
        }
        Ok(leaf)
    }

    // None means an inline record. A named (potentially nullable) record
    // can instead be a typed leaf without recursively unfolding its shape.
    fn state_shape_at(&self, root: &str, fields: &[String]) -> Result<Option<&str>> {
        let state = self.environment.get(root).ok_or("undeclared state root")?;
        if state.kind != "state" || state.source.is_empty() { return Err("invalid state source".into()); }
        if state.fields.is_empty() {
            let domain = state.ty.as_deref().ok_or("undeclared state field")?;
            return self.declared_field_type(domain, fields.iter().map(String::as_str))
                .map(Some).ok_or_else(|| "undeclared state field".into());
        }
        let mut members = &state.fields;
        for (i, name) in fields.iter().enumerate() {
            match members.get(name).ok_or("undeclared state field")? {
                Field::Record(record) => members = record,
                Field::Leaf(ty) if i + 1 == fields.len() => return Ok(Some(ty.as_str())),
                Field::Leaf(ty) => {
                    if !self.types.contains_key(ty.trim_end_matches('?')) {
                        return Err("scalar field has no members".into());
                    }
                    return self.declared_field_type(ty, fields[i + 1..].iter().map(String::as_str))
                        .map(Some).ok_or_else(|| "undeclared state field".into());
                }
            }
        }
        Ok(None)
    }

    /// The adapter transport cannot relabel a checked business value.
    pub(super) fn validate_carrier_domains(
        &self, method: &JavaMethod, params: &[super::Ty], returns: &super::Ty,
    ) -> Result<()> {
        for ((_, name), ty) in method.params().iter().zip(params) {
            let (_, domain) = self.binding(name)?;
            if !self.matches_adapter_domain(ty, domain) {
                return Err(format!("adapter parameter {name} disagrees with its declared domain {domain}").into());
            }
        }
        if !self.matches_adapter_domain(returns, &self.returns) {
            return Err(format!("adapter result disagrees with declared domain {}", self.returns).into());
        }
        Ok(())
    }

    fn matches_adapter_domain(&self, ty: &super::Ty, domain: &str) -> bool {
        use super::Ty;
        match ty {
            Ty::Unit => domain == "void",
            Ty::Integer | Ty::Decimal => domain == "number",
            Ty::Bool => domain == "bool",
            Ty::CalendarDate => domain == "date",
            Ty::Text => matches!(domain, "text" | "text?"),
            Ty::UnknownInput => domain == "unknown",
            Ty::OrderedTextMap => domain == "map<text,text>",
            Ty::Seq(item) | Ty::Vector(item) => domain.strip_suffix("[]")
                .is_some_and(|item_domain| self.matches_adapter_domain(item, item_domain)),
            Ty::Record(fields) => self.types.get(domain).is_some_and(|record|
                record.fields.len() == fields.len() && fields.iter().all(|(name, ty)|
                    record.fields.get(name).is_some_and(|domain| self.matches_adapter_domain(ty, domain)))),
            Ty::Ref | Ty::Task(_) | Ty::LocalRelation | Ty::LocalTextMap => false,
        }
    }

    fn binding(&self, name: &str) -> Result<(String, &str)> {
        let Some(path) = self.bindings.get(name) else {
            let parameter = self.parameters.get(name).ok_or_else(|| format!("undeclared input {name}"))?;
            return Ok((self.qualified("P", name), &parameter.ty));
        };
        let ty = self.state_field(&path.root, &path.fields)?;
        Ok((format!("{}.{}", self.qualified("E", &path.root), path.fields.join(".")), ty))
    }

    fn declared_field_type<'a, 'path>(&'a self, mut ty: &'a str, path: impl Iterator<Item=&'path str>) -> Option<&'a str> {
        for name in path {
            ty = self.types.get(ty.trim_end_matches('?'))?.fields.get(name)?;
        }
        Some(ty)
    }

    fn has_field<'a>(&'a self, ty: &'a str, path: impl Iterator<Item=&'a str>) -> bool {
        self.declared_field_type(ty, path).is_some()
    }

    pub(super) fn qualified(&self, kind: &str, name: &str) -> String {
        if self.parameters.contains_key(name) && self.environment.contains_key(name) {
            format!("{kind}.{name}")
        } else { name.into() }
    }

    pub(super) fn external_type(&self, port: &str) -> Result<(&str,super::Ty)> {
        let name = self.external_results.get(port).ok_or("missing business return type for port")?;
        fn resolve(context:&EntryContext, name:&str, seen:&mut std::collections::BTreeSet<String>)->Result<super::Ty> {
            use super::Ty;
            if let Some(item) = name.strip_suffix("[]") { return Ok(Ty::Vector(Box::new(resolve(context,item,seen)?))); }
            match name { "bool"=>Ok(Ty::Bool), "text"=>Ok(Ty::Text), "number"=>Ok(Ty::Integer), "void"=>Ok(Ty::Unit),
                _ => {
                    let record = context.types.get(name).ok_or("missing external type declaration")?;
                    if !seen.insert(name.to_owned()) { return Err("recursive external value type".into()); }
                    let fields = record.fields.iter().map(|(key,ty)| Ok((key.clone(),resolve(context,ty,seen)?))).collect::<Result<_>>()?;
                    seen.remove(name); Ok(Ty::Record(fields))
                }
            }
        }
        Ok((name, resolve(self,name,&mut Default::default())?))
    }

    pub(super) fn host_names(&self) -> BTreeMap<String,String> {
        self.parameters.keys().map(|name| (super::host_input_symbol(name), self.qualified("P",name))).collect()
    }

    pub(super) fn source_parameter_path(&self, name: &str) -> Option<String> {
        let (root, tail) = name.split_once('.')?;
        let source_type = self.parameter_type(root)?.trim_end_matches('?');
        self.has_field(source_type, tail.split('.')).then(|| format!("{}.{tail}", self.qualified("P", root)))
    }

    pub(super) fn source_identity_names(&self, result: &ExecutionResult) -> BTreeMap<i64, String> {
        result.source_identity_parameters.iter().filter_map(|(id, root)| {
            let (parameter, fields) = root.split_once('.').map_or((root.as_str(), None), |(name, fields)| (name, Some(fields)));
            let ty = self.parameter_type(parameter)?;
            let ty = match fields {
                Some(fields) => self.declared_field_type(ty, fields.split('.'))?,
                None => ty,
            };
            let ty = ty.trim_end_matches('?');
            // Keep root integer-array aliases as well as declared nested
            // record paths. Nested array identity is a separate contract.
            if !self.types.contains_key(ty) && !(fields.is_none() && ty == "number[]") { return None; }
            let public_root = self.qualified("P", parameter);
            let display = fields.map_or_else(|| public_root.clone(), |tail| format!("{public_root}.{tail}"));
            Some((*id, display))
        }).chain(result.source_identity_states.iter().filter_map(|(id, binding)| {
            let root = result.source_state_bindings.get(binding)?;
            let state = self.environment.get(root)?;
            (!state.fields.is_empty() || state.ty.as_deref().is_some_and(|ty| self.types.contains_key(ty.trim_end_matches('?'))))
                .then(|| (*id, self.qualified("E", root)))
        })).collect()
    }

    pub(super) fn source_identity_keys(
        &self, result: &ExecutionResult, names: &BTreeMap<i64, String>,
    ) -> BTreeMap<i64, math::HeapBindingIdentity> {
        names.keys().map(|&id| {
            let identity = if let Some(&binding) = result.source_identity_states.get(&id) {
                math::HeapBindingIdentity::EntrySnapshot(vec![result.source_state_bindings[&binding].clone()])
            } else {
                math::HeapBindingIdentity::Parameter(
                    result.source_identity_parameters[&id].split('.').map(str::to_owned).collect())
            };
            (id, identity)
        }).collect()
    }

    pub(super) fn source_state_path(&self, root: &str, path: &[String]) -> bool {
        self.state_shape_at(root, path).is_ok()
    }

    pub(super) fn state_name(&self, path: &[String]) -> Result<String> {
        let (root,fields) = path.split_first().ok_or("missing target")?;
        if fields.is_empty() {
            return Ok(self.qualified("E", root));
        }
        self.state_field(root, fields)?;
        Ok(format!("{}.{}",self.qualified("E",root),fields.join(".")))
    }

    pub(super) fn has_record_array_integer_field(&self, name: &str, field: &str) -> bool {
        self.parameter_type(name).and_then(|domain| domain.strip_suffix("[]"))
            .and_then(|record| self.declared_field_type(record, field.split('.')))
            .is_some_and(|domain| domain == "number")
    }

    pub(super) fn record_array_symbols(&self) -> std::collections::BTreeSet<String> {
        self.parameters.iter().filter_map(|(name, param)|
            self.types.contains_key(param.ty.strip_suffix("[]")?).then(|| name.clone())).collect()
    }

    pub(super) fn scalar_array_symbols(&self) -> std::collections::BTreeSet<String> {
        let mut arrays = self.parameters.iter().filter(|(_, p)| p.ty == "number[]")
            .map(|(name, _)| name.clone()).collect::<std::collections::BTreeSet<_>>();
        arrays.extend(self.environment.iter().filter(|(_, state)| state.ty.as_deref() == Some("number[]"))
            .map(|(name, _)| super::source_state_symbol(name)));
        arrays
    }

    fn add_initial_state_names(&self, names: &mut BTreeMap<String, String>) {
        for (root, state) in &self.environment {
            if state.ty.as_deref().is_some_and(|ty| matches!(ty, "number" | "bool" | "text" | "number[]")) {
                names.insert(super::source_state_symbol(root), self.qualified("E", root));
            }
        }
    }

    pub(super) fn acquired_state_path(&self, result: &ExecutionResult, id: u64, path: &[String]) -> Option<String> {
        let root = result.source_state_bindings.get(&id)?;
        let label = self.qualified("E", root);
        Some(std::iter::once(label.as_str()).chain(path.iter().map(String::as_str)).collect::<Vec<_>>().join("."))
    }

    pub(super) fn publish_temporals(
        &self, result: &ExecutionResult, returned: Option<&Expr>, names: &BTreeMap<String, String>,
        acquired_path: impl Fn(u64, &[String]) -> Option<String>,
    ) -> Result<super::temporal_graph::Publication> {
        let domains = self.parameters.iter().map(|(n, p)| (n.clone(), p.ty.clone())).collect();
        super::temporal_graph::publish(&result.temporal_definitions, returned,
            &self.scalar_array_symbols(), &domains, names, acquired_path)
    }

    fn certifies_composite_return(&self, typed: &super::Ty) -> bool {
        (*typed == super::Ty::Vector(Box::new(super::Ty::Integer)) && self.returns == "number[]")
            || (*typed == super::Ty::OrderedTextMap && self.returns == "map<text,text>")
    }

    fn render_source(&self, project: &super::Project, result: &ExecutionResult, method: &JavaMethod) -> Result<String> {
        let events = result.realtime_effects.events();
        let certified_composite_return = self.certifies_composite_return(&result.entry_return_type);
        let has_temporals = !result.temporal_definitions.is_empty();
        let total_return = if events.is_empty() || has_temporals {
            result.terminal.total_return_value()
        } else {
            None
        };
        if events.is_empty() {
            if let Some(returned) = total_return {
                return self.render_output(BusinessOutput::Pure(returned.clone()), result, method, certified_composite_return);
            }
        }
        let mut names = self.host_names();
        self.add_initial_state_names(&mut names);
        for (_, name) in method.params() {
            names.insert(name.clone(), self.binding(name)?.0);
        }
        let observations = super::business_event::observations(project, self, result, names)?;
        self.render_output(BusinessOutput::Observed(observations), result, method, certified_composite_return)
    }

    fn render_output(&self, output: BusinessOutput, result: &ExecutionResult, method: &JavaMethod, certified_composite_return: bool) -> Result<String> {
        let mut names = BTreeMap::new();
        for (_, name) in method.params() {
            names.insert(name.clone(), self.binding(name)?.0);
        }
        for name in self.parameters.keys() {
            names.insert(super::host_input_symbol(name), self.qualified("P", name));
        }
        self.add_initial_state_names(&mut names);
        let states = result.source_state_bindings.iter().map(|(id, root)| (*id, self.qualified("E", root))).collect::<BTreeMap<_,_>>();
        let arrays = self.scalar_array_symbols();
        let nested_arrays = self.parameters.iter().filter(|(_, param)| param.ty == "number[][]")
            .map(|(name, _)| name.clone()).collect::<std::collections::BTreeSet<_>>();
        let record_inputs = self.parameters.iter().filter_map(|(name, p)| {
            let record = self.types.get(p.ty.strip_suffix("[]")?)?;
            Some((name.clone(), record.fields.keys().cloned().collect::<std::collections::BTreeSet<_>>()))
        }).collect::<BTreeMap<_, _>>();
        let identities = self.source_identity_names(result);
        let heap_keys = self.source_identity_keys(result, &identities);
        let publication = match &output {
            BusinessOutput::Pure(returned) if !result.temporal_definitions.is_empty() => {
                let returned = if let Some(fields) = &result.source_record_return {
                    math::builder::array_lit(fields.iter().map(|(_, expression)| expression.clone()).collect())
                } else {
                    returned.clone()
                };
                self.publish_temporals(result, Some(&returned), &names, |id, path| self.acquired_state_path(result, id, path))?
            }
            _ => super::temporal_graph::Publication::default(),
        };
        let definition_names = &publication.aliases;
        let render_value = |expression: &Expr, record_fields: Option<&[(String, Expr)]>| -> Result<String> {
            if !result.temporal_definitions.is_empty() {
                super::temporal_graph::validate_reads(expression, &result.temporal_definitions)?;
            }
            let mut closed = true;
            let mut scalar_element = false;
            let mut record_rows = BTreeMap::<String, std::collections::BTreeSet<String>>::new();
            let mut correlated = std::collections::BTreeSet::new();
            expression.walk(&mut |node| {
                if let Some((kind, relation, select, _)) = node.as_relation_aggregate() {
                    use super::business_relation::{correlated_array_relation, relation_base};
                    use math::RelationAggregateKind::Sum;
                    if kind == Sum {
                        if let crate::kernel_input::RelationExpr::Join { left, right, .. } = relation {
                            for (table, alias) in [left, right] {
                                if let Some(columns) = result.source_tables.get(&table.name) {
                                    record_rows.entry(alias.clone()).or_default().extend(columns.keys().cloned());
                                }
                            }
                        }
                        if let (Some((source, group)), Some((Sum, inner, _, _))) =
                            (relation_base(relation), select.and_then(Expr::as_relation_aggregate)) {
                            if nested_arrays.contains(source) {
                                if let Some((from, element)) = correlated_array_relation(inner) {
                                    if from == group && group != element {
                                        correlated.insert((group.into(), element.into()));
                                    }
                                }
                            }
                        }
                    }
                    scalar_element |= super::business_relation::is_scalar_array_relation(relation, &arrays);
                    if let Some((name, alias)) = super::business_relation::relation_base(relation) {
                        if let Some(fields) = record_inputs.get(name) {
                            record_rows.entry(alias.into()).or_default().extend(fields.iter().cloned());
                        }
                    }
                }
            });
            let mut inputs = names.clone();
            expression.walk(&mut |node| {
                closed &= match node.as_sym_var() {
                    Some(SymVarOrigin::Parameter { name, .. }) => {
                        if let Some(display) = self.source_parameter_path(name) {
                            inputs.insert(name.clone(), display);
                        }
                        inputs.contains_key(name)
                    }
                    Some(SymVarOrigin::External { binding_id, path, .. }) => {
                        result.source_state_bindings.get(binding_id)
                            .is_some_and(|root| self.source_state_path(root, path))
                    }
                    Some(SymVarOrigin::Temporal { .. }) => super::temporal_definition_id(node)
                        .is_some_and(|id| definition_names.contains_key(&id)),
                    None => super::business_relation::is_table_expression(node, &result.source_tables, &arrays, scalar_element, &nested_arrays, &correlated, &record_inputs, &record_rows)
                        || ((certified_composite_return || record_fields.is_some()) && node.as_array_lit().is_some())
                        || (certified_composite_return && self.returns == "map<text,text>"
                            && math::domains::composite::ordered_text_map_entries(node).is_some())
                        || super::record_choice_projection(node).is_some()
                        || node.as_heap_ref().is_some_and(|id| identities.contains_key(&id))
                        || node.as_lit().is_some() || node.as_binop().is_some() || node.as_unary_op().is_some()
                        || node.as_case_when().is_some() || math::domains::decimal::is_decimal(node)
                        || math::domains::integer_quotient::is_quotient(node)
                        || math::domains::composite::vector_length_input(node)
                            .and_then(math::domains::composite::input_name).is_some_and(|name| arrays.contains(name))
                        || math::domains::composite::input_name(node).is_some_and(|name| arrays.contains(name))
                        || node.as_call().is_some_and(|(key,_)| matches!(math::func_name(key).as_str(),crate::normalized_text::DEFAULT_IF_BLANK_KEY | crate::normalized_text::TRIM_KEY)),
                }
            });
            if !closed { return Err("[NORM-BUSINESS-EXPRESSION] expression has an unsupported operation or input".into()); }
            // The boundary checks both identity and definition references
            // before passing their stable identities to the typed normalizer.
            let mut ctx = crate::FormatCtx::for_business(inputs, states.clone(), &result.source_state_bindings);
            let expression = if expression.has_heap_ref() || !definition_names.is_empty() || !states.is_empty() {
                let public = crate::canonical_names::CanonicalNames {
                    heap: &heap_keys,
                    external: (!states.is_empty()).then_some(ctx.observation_identities()),
                    total_temporals: Some(&publication.total), reductions: Some(&publication.reductions), temporals: (!definition_names.is_empty()).then_some(definition_names),
                };
                math::try_normalize_with(expression.clone(), &public)
                    .map_err(|error| error.to_string())?
            } else {
                expression.clone()
            };
            if !result.temporal_definitions.is_empty() {
                super::temporal_graph::validate_reads(&expression, &result.temporal_definitions)?;
            }
            ctx.set_temporal_names(definition_names.clone());
            ctx.set_relation_slots(publication.slots.clone());
            ctx.set_reductions(publication.reductions.clone());
            ctx.set_scalar_arrays(arrays.clone());
            ctx.declare_nullable_columns(&result.source_tables);
            ctx.set_nested_arrays(nested_arrays.clone());
            ctx.set_record_arrays(record_inputs.keys().cloned().collect());
            ctx.set_business_identities(identities.clone(), heap_keys.clone());
            Ok(if let Some(fields) = record_fields {
                crate::formatting::sql::business_value::format_record(&expression, fields, &mut ctx)
            } else {
                crate::formatting::sql::business_value::format_value(&expression, &mut ctx)
            })
        };

        let value = match &output {
            BusinessOutput::Observed(observations) => observations.returned.clone(),
            BusinessOutput::Pure(returned) => Some(
                if let Some(record) = &result.source_record_return {
                    self.types.get(&self.returns).ok_or("result record type is undeclared")?;
                    let aggregate = math::builder::array_lit(record.iter().map(|(_, value)| value.clone()).collect());
                    render_value(&aggregate, Some(record))?
                } else if self.returns == "void" {
                    if !returned.is_null() { return Err("non-void return for void entry".into()); }
                    "void".into()
                } else {
                    render_value(returned, None)?
                }
            ),
        };
        let observations = match &output {
            BusinessOutput::Observed(observations) => Some(observations),
            BusinessOutput::Pure(_) => None,
        };
        let mut doc = json!({"schema": super::kernel_result::PERS_SCHEMA, "entry": {"function": self.function, "returns": self.returns},
            "P": {"parameters": self.parameters}});
        if let Some(value) = value { doc["R"] = json!(value); }
        if !self.environment.is_empty() {
            let environment = self.environment.iter().map(|(name, state)| {
                let rendered = match state.ty.as_ref().and_then(|ty| self.types.get(ty)) {
                    // The public root is a record, not a second named value
                    // declaration. Do not expand named nested/recursive leaves.
                    Some(record) if state.fields.is_empty() =>
                        json!({"kind": state.kind, "source": state.source, "fields": record.fields}),
                    _ => json!(state),
                };
                (name, rendered)
            }).collect::<BTreeMap<_, _>>();
            doc["E"] = json!(environment);
        }
        let tables = observations.map(|o| &o.tables).unwrap_or(&result.source_tables);
        for (table, columns) in tables {
            doc["E"][table] = json!({"kind":"table", "source":table, "columns":columns});
        }
        if let Some(observations) = observations {
            for (identity, acquisition) in &observations.acquisitions {
                doc["E"][identity] = acquisition.clone();
            }
            if !observations.effects.is_empty() { doc["S"] = json!(observations.effects); }
            if !observations.throws.is_empty() { doc["throws"] = json!(observations.throws); }
        }
        // Named types remain only where referenced by the emitted PERS.
        // The internal root type stays available for typed lookup and identity.
        let definitions = self.published_types(&doc);
        if !definitions.is_empty() { doc["types"] = json!(definitions); }
        serde_yaml::to_string(&doc).map_err(|e| e.to_string())
    }

    fn published_types(&self, doc: &serde_json::Value) -> BTreeMap<String, &Record> {
        fn field_types(node: &serde_json::Value, pending: &mut Vec<String>) {
            if let Some(leaf) = node.as_str() {
                pending.push(leaf.to_owned());
            } else if let Some(fields) = node.as_object() {
                for field in fields.values() { field_types(field, pending); }
            }
        }

        let mut pending = vec![self.returns.clone()];
        pending.extend(self.parameters.values().map(|p| p.ty.clone()));
        if let Some(environment) = doc["E"].as_object() {
            for state in environment.values() {
                for key in ["type", "fields", "columns"] { field_types(&state[key], &mut pending); }
            }
        }
        if let Some(effects) = doc["S"].as_array() {
            for operation in effects {
                field_types(&operation["call"]["returns"], &mut pending);
            }
        }
        let mut published = BTreeMap::new();
        while let Some(domain) = pending.pop() {
            let ty = domain.trim_end_matches('?').trim_end_matches("[]");
            if let Some(record) = self.types.get(ty) {
                if !published.contains_key(ty) {
                    published.insert(ty.to_owned(), record);
                    pending.extend(record.fields.values().cloned());
                }
            }
        }
        published
    }
}
