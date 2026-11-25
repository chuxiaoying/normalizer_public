//! Source-level entry identity and bindings for the business PERS document.
use super::{identifier, Loaded, ExecutionResult, Result};
use math::{Expr, SymVarOrigin};
use serde::{Deserialize, Serialize};
use serde_json::json;
use simulator_parser::{JavaMethod, JavaType};
use std::{collections::BTreeMap, path::Path};

pub(super) struct BusinessObservations {
    pub(super) returned: String,
    pub(super) tables: BTreeMap<String, BTreeMap<String, String>>,
    pub(super) effects: Vec<serde_json::Value>,
    pub(super) throws: Vec<serde_json::Value>,
    pub(super) definitions: Vec<serde_json::Value>,
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
    bindings: BTreeMap<String, Vec<String>>,
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
        t if crate::normalized::exact_decimal_type(t) => Ok("number".into()),
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
        let mut methods = class.methods().iter().filter(|m| m.name() == name);
        let method = methods.next().ok_or("entry method not found")?;
        if methods.next().is_some() { return Err(format!("ambiguous entry {entry}")); }
        Ok((class, method))
    }

    pub fn source_pers(&self, root: &Path, entry: &str) -> Result<String> {
        let (class, method) = self.entry_method(entry)?;
        let source = Path::new(class.source_file().ok_or("missing entry source location")?);
        let path = source.strip_prefix(root).map_err(|_| "entry file outside project")?;
        let result = self.normalize(entry)?;
        let signature = method.params().iter().map(|(ty, _)| ty).chain([method.return_type()]);
        let mut types = BTreeMap::new();
        let mut environment = BTreeMap::new();
        let mut pending = signature.collect::<Vec<_>>();
        let mut external_results = BTreeMap::new();
        for event in result.realtime_effects.events() {
            if let super::realtime_state::EffectOperation::Port(id) = &event.operation {
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
            environment.insert(name.clone(), State {
                kind: "state".into(), source: format!("{}#{}.{name}", path.display(), class.full_name()),
                ty: Some(ty), fields: BTreeMap::new(),
            });
        }
        while let Some(ty) = pending.pop() {
            if let JavaType::Array(item) = ty { pending.push(item); continue; }
            let JavaType::Class(name) = ty else { continue };
            if source_type(ty)? == "text" || crate::normalized::exact_decimal_type(ty) || types.contains_key(name) { continue; }
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
            function: format!("{}#{entry}", path.display()),
            types,
            returns: {
                let mut domain = source_type(method.return_type())?;
                if crate::normalized::nullable_text_return(class, method) { domain.push('?'); }
                domain
            },
            parameters: method.params().iter().enumerate().map(|(position, (ty, name))| {
                let mut domain = source_type(ty)?.to_owned();
                for annotation in method.param_annotations(name) {
                    if annotation.name() != "normalizer.annotations.Nullable" || !annotation.args().is_empty() || !matches!(ty, JavaType::Class(_)) {
                        return Err(format!("unsupported source parameter annotation {}", annotation.name()));
                    }
                    domain.push('?');
                }
                Ok((name.clone(), Parameter { position, ty: domain }))
            }).collect::<Result<_>>()?,
            environment,
            bindings: BTreeMap::new(),
        };
        context.render_source(&self.project, &result, method)
    }

    pub fn project_pers(&self, entry: &str) -> Result<String> {
        let result = self.normalize(entry)?;
        match self.project.entry_contexts.get(entry) {
            Some(context) => context.render_source(&self.project, &result, self.entry_method(entry)?.1),
            None => Err("missing business entry context".into()),
        }
    }
}

impl EntryContext {
    pub(super) fn return_type(&self) -> &str { &self.returns }

    pub(super) fn parameter_type(&self, name: &str) -> Option<&str> {
        self.parameters.get(name).map(|p| p.ty.as_str())
    }

    pub(super) fn state_field(&self, root: &str, fields: &[String]) -> Result<&str> {
        if fields.is_empty() { return Err("state path requires a field".into()); }
        let state = self.environment.get(root).ok_or("undeclared state root")?;
        if state.kind != "state" || state.source.is_empty() { return Err("invalid state source".into()); }
        if state.fields.is_empty() {
            let mut domain = state.ty.as_deref().ok_or("undeclared state field")?;
            for field in fields {
                domain = self.types.get(domain).and_then(|record| record.fields.get(field))
                    .ok_or("undeclared state field")?;
            }
            return if matches!(domain.trim_end_matches('?'), "number" | "bool" | "text") {
                Ok(domain)
            } else { Err("expected a scalar state field".into()) };
        }
        let mut field = state.fields.get(&fields[0]).ok_or("undeclared state field")?;
        for name in &fields[1..] {
            let Field::Record(record) = field else { return Err("scalar field has no members".into()) };
            field = record.get(name).ok_or("undeclared state field")?;
        }
        let Field::Leaf(ty) = field else { return Err("expected a scalar state field".into()) };
        Ok(ty)
    }

    fn binding(&self, name: &str) -> Result<(String, &str)> {
        let Some(path) = self.bindings.get(name) else {
            let parameter = self.parameters.get(name).ok_or_else(|| format!("undeclared input {name}"))?;
            return Ok((self.qualified("P", name), &parameter.ty));
        };
        match path.as_slice() {
            [kind, root] if kind == "P" => {
                let parameter = self.parameters.get(root).ok_or("unbound parameter")?;
                Ok((self.qualified(kind, root), &parameter.ty))
            }
            [kind, root, fields @ ..] if kind == "E" && !fields.is_empty() => {
                let ty = self.state_field(root, fields)?;
                Ok((format!("{}.{}", self.qualified(kind, root), fields.join(".")), ty))
            }
            _ => Err("invalid business source binding".into())
        }
    }

    fn has_field<'a>(&'a self, mut ty: &'a str, path: impl Iterator<Item=&'a str>) -> bool where Self: 'a {
        for name in path {
            let Some(next) = self.types.get(ty).and_then(|record| record.fields.get(name)) else { return false; };
            ty = next;
        }
        true
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
            let ty = self.parameter_type(root)?.trim_end_matches('?');
            self.types.contains_key(ty).then(|| (*id, self.qualified("P", root)))
        }).chain(result.source_identity_states.iter().filter_map(|(id, binding)| {
            let root = result.source_state_bindings.get(binding)?;
            let state = self.environment.get(root)?;
            self.types.contains_key(state.ty.as_deref()?).then(|| (*id, self.qualified("E", root)))
        })).collect()
    }

    pub(super) fn source_state_path(&self, root: &str, path: &[String]) -> bool {
        self.environment.get(root).and_then(|state| state.ty.as_deref())
            .is_some_and(|ty| self.has_field(ty, path.iter().map(String::as_str)))
    }

    pub(super) fn state_name(&self, path: &[String]) -> Result<String> {
        let (root,fields) = path.split_first().ok_or("missing target")?;
        if fields.is_empty() {
            let state = self.environment.get(root).ok_or("undeclared state root")?;
            let domain = state.ty.as_deref().ok_or("target requires scalar state")?;
            if state.kind != "state" || !matches!(domain.trim_end_matches('?'), "number" | "bool" | "text") {
                return Err("target requires scalar state".into());
            }
            return Ok(self.qualified("E", root));
        }
        self.state_field(root, fields)?;
        Ok(format!("{}.{}",self.qualified("E",root),fields.join(".")))
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
        if !self.source_state_path(root, path) {
            return None;
        }
        let label = self.qualified("E", root);
        Some(std::iter::once(label.as_str()).chain(path.iter().map(String::as_str)).collect::<Vec<_>>().join("."))
    }

    pub(super) fn publish_temporals(
        &self, result: &ExecutionResult, returned: &Expr, names: &BTreeMap<String, String>,
        acquired_path: impl Fn(u64, &[String]) -> Option<String>,
    ) -> Result<super::temporal_graph::Publication> {
        let domains = self.parameters.iter().map(|(n, p)| (n.clone(), p.ty.clone())).collect();
        super::temporal_graph::publish(&result.temporal_definitions, returned,
            &self.scalar_array_symbols(), &domains, names, acquired_path)
    }

    fn render_source(&self, project: &super::Project, result: &ExecutionResult, method: &JavaMethod) -> Result<String> {
        let events = result.realtime_effects.events();
        let typed = project.result_type(method.return_type())?;
        let certified_composite_return = (typed == super::Ty::Vector(Box::new(super::Ty::Integer))
            && self.returns == "number[]") || (typed == super::Ty::OrderedTextMap
            && self.returns == "map<text,text>");
        if !result.temporal_definitions.is_empty()
            && (result.realtime_completion.return_value().is_none()
                || result.source_record_return.is_some() && !events.is_empty())
        {
            return Err("temporal publication requires a closed normal entry; record results cannot mix with effects".into());
        }
        if events.is_empty() && result.realtime_completion.return_value().is_some() {
            return self.render_observations(None, result, method, certified_composite_return);
        }
        let mut names = self.host_names();
        self.add_initial_state_names(&mut names);
        for (_, name) in method.params() {
            names.insert(name.clone(), self.binding(name)?.0);
        }
        let observations = super::business_event::observations(project, self, result, names)?;
        self.render_observations(Some(observations), result, method, certified_composite_return)
    }

    fn render_observations(&self, observations:Option<BusinessObservations>, result: &ExecutionResult, method: &JavaMethod, certified_composite_return: bool) -> Result<String> {
        if self.function.is_empty() || !self.function.contains('#') { return Err("missing business entry identity".into()); }
        let mut positions: Vec<_> = self.parameters.values().map(|p| p.position).collect();
        positions.sort_unstable();
        if !positions.into_iter().eq(0..self.parameters.len()) { return Err("invalid business parameter positions".into()); }
        let mut names = BTreeMap::new();
        for (ty, name) in method.params() {
            let (display, domain) = self.binding(name)?;
            check_domain(ty, domain)?;
            names.insert(name.clone(), display);
        }
        if self.bindings.keys().any(|k| !names.contains_key(k))
            || self.bindings.values().flatten().any(|part| !identifier(part)) {
            return Err("invalid business input mapping".into());
        }
        for name in self.parameters.keys() {
            names.insert(super::host_input_symbol(name), self.qualified("P", name));
        }
        if !certified_composite_return { check_domain(method.return_type(), &self.returns)?; }
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
        let publication = if observations.is_some() || result.temporal_definitions.is_empty() {
            super::temporal_graph::Publication::default()
        } else {
            let returned = if let Some(fields) = &result.source_record_return {
                math::builder::array_lit(fields.iter().map(|(_, expression)| expression.clone()).collect())
            } else {
                result.business_return()?
            };
            self.publish_temporals(result, &returned, &names, |id, path| self.acquired_state_path(result, id, path))?
        };
        let definition_names = &publication.aliases;
        let render_value = |expression: &Expr| -> Result<String> {
            if !result.temporal_definitions.is_empty() {
                super::temporal_graph::validate_reads(expression, &result.temporal_definitions)?;
            }
            let mut closed = true;
            let mut scalar_element = false;
            let mut record_rows = BTreeMap::<String, std::collections::BTreeSet<String>>::new();
            let mut correlated = std::collections::BTreeSet::new();
            let mut group_tuples = Vec::new();
            let mut join_tables = std::collections::BTreeSet::new();
            expression.walk(&mut |node| {
                if let Some((kind, relation, select, _)) = node.as_relation_aggregate() {
                    use super::business_relation::{correlated_array_relation, relation_base};
                    use math::RelationAggregateKind::Sum;
                    if kind == Sum {
                        if let Some(join) = select.and_then(|p| super::business_relation::inner_join_count(relation, p)) {
                            for (table, alias) in [join.left, join.right] {
                                if let Some(columns) = result.source_tables.get(table) {
                                    join_tables.insert((table.into(), alias.into()));
                                    record_rows.entry(alias.into()).or_default().extend(columns.keys().cloned());
                                }
                            }
                        }
                        if let Some((table, keys)) = super::business_relation::grouped_table(relation) {
                            if select == Some(&math::builder::lit_int(1)) && result.source_tables.contains_key(table) {
                                group_tuples.push(keys.to_vec());
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
                            .and_then(|root| self.environment.get(root))
                            .and_then(|state| state.ty.as_ref())
                            .is_some_and(|ty| self.has_field(ty, path.iter().map(String::as_str)))
                    }
                    Some(SymVarOrigin::Temporal { .. }) => super::temporal_definition_id(node)
                        .is_some_and(|id| definition_names.contains_key(&id)),
                    None => super::business_relation::is_table_expression(node, &result.source_tables, &arrays, scalar_element, &nested_arrays, &correlated, &join_tables, &record_inputs, &record_rows)
                        || node.as_array_lit().is_some_and(|items| certified_composite_return || group_tuples.iter().any(|keys| keys == items))
                        || (certified_composite_return && self.returns == "map<text,text>"
                            && math::domains::composite::ordered_text_map_entries(node).is_some())
                        || super::record_choice_projection(node).is_some()
                        || node.as_heap_ref().is_some_and(|id| identities.contains_key(&id))
                        || node.as_lit().is_some() || node.as_binop().is_some() || node.as_unary_op().is_some()
                        || node.as_case_when().is_some() || math::domains::decimal::is_decimal(node)
                        || math::domains::composite::vector_length_input(node)
                            .and_then(math::domains::composite::input_name).is_some_and(|name| arrays.contains(name))
                        || math::domains::composite::input_name(node).is_some_and(|name| arrays.contains(name))
                        || node.as_call().is_some_and(|(key,_)| matches!(math::func_name(key).as_str(),crate::normalized_text::KEY | crate::normalized_text::TRIM_KEY)),
                }
            });
            if !closed { return Err("[NORM-BUSINESS-EXPRESSION] expression has an unsupported operation or input".into()); }
            // The boundary checks both identity and definition references
            // before passing their public names to the typed math normalizer.
            let expression = if expression.has_heap_ref() || !definition_names.is_empty() || !states.is_empty() {
                let public = crate::canonical_names::CanonicalNames {
                    heap: &identities,
                    external: (!states.is_empty()).then_some(&states),
                    total_temporals: Some(&publication.total), temporals: (!definition_names.is_empty()).then_some(definition_names),
                };
                math::try_normalize_with(expression.clone(), &public)
                    .map_err(|error| error.to_string())?
            } else {
                expression.clone()
            };
            if !result.temporal_definitions.is_empty() {
                super::temporal_graph::validate_reads(&expression, &result.temporal_definitions)?;
            }
            let mut ctx = crate::FormatCtx::for_business(inputs, states.clone());
            ctx.set_temporal_names(definition_names.clone());
            ctx.set_scalar_arrays(arrays.clone());
            ctx.declare_nullable_columns(&result.source_tables);
            ctx.set_nested_arrays(nested_arrays.clone());
            ctx.set_record_arrays(record_inputs.keys().cloned().collect());
            ctx.set_business_identities(identities.clone());
            Ok(crate::formatting::sql::business_value::format_value(&expression, &mut ctx))
        };

        let value = if let Some(observations) = &observations { observations.returned.clone() } else if let Some(record) = &result.source_record_return {
            result.business_return()?;
            let definition = self.types.get(&self.returns).ok_or("result record type is undeclared")?;
            if record.len() != definition.fields.len() || record.iter().any(|(key,_)| !definition.fields.contains_key(key)) {
                return Err("result record fields differ from boundary type".into());
            }
            let fields = record.iter().map(|(name, value)| {
                let rendered = render_value(value)?.replace('\n', "\n  ");
                Ok(format!("  {name}: {rendered}"))
            }).collect::<Result<Vec<_>>>()?;
            format!("{{\n{}\n}}", fields.join(",\n"))
        } else { render_value(&result.business_return()?)? };
        let mut doc = json!({"schema": super::kernel_result::PERS_SCHEMA, "entry": {"function": self.function, "returns": self.returns},
            "P": {"parameters": self.parameters}, "R": value});
        let definitions = observations.as_ref().map_or(&publication.rows, |o| &o.definitions);
        if !definitions.is_empty() { doc["T"] = json!(definitions); }
        if !self.types.is_empty() { doc["types"] = json!(self.types); }
        if !self.environment.is_empty() { doc["E"] = json!(self.environment); }
        let tables = observations.as_ref().map(|o| &o.tables).unwrap_or(&result.source_tables);
        for (table, columns) in tables {
            if self.parameters.contains_key(table) || self.environment.contains_key(table) {
                return Err(format!("unresolved table source name collision: {table}"));
            }
            doc["E"][table] = json!({"kind":"table", "source":table, "columns":columns});
        }
        if let Some(observations) = observations {
            doc["S"] = json!(observations.effects);
            if !observations.throws.is_empty() { doc["throws"] = json!(observations.throws); }
        }
        serde_yaml::to_string(&doc).map_err(|e| e.to_string())
    }
}

fn check_domain(ty: &JavaType, domain: &str) -> Result<()> {
    let base = domain.strip_suffix('?').unwrap_or(domain);
    if source_type(ty)? == base { Ok(()) }
    else { Err(format!("carrier {ty:?} is incompatible with business type {domain}")) }
}
