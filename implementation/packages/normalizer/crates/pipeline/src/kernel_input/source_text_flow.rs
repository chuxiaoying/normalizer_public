//! Closed native TypeScript decisions over acquired inputs. The frontend
//! checks lexical binding and types; execution establishes path-local proofs.
use super::*;
use realtime_state::Completion;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum NativeTextFlow {
    If { condition: TextTest, then: Box<Self>, otherwise: Box<Self> },
    Return { value: TextValue },
    Throw { #[serde(rename = "type")] class: NativeError, message: WireText },
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) enum NativeError { Error, TypeError }

impl NativeError {
    fn name(&self) -> &'static str {
        match self { Self::Error => "Error", Self::TypeError => "TypeError" }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum TextTest {
    Type { name: String },
    Equal { name: String, literal: WireText },
}

impl TextTest {
    fn name(&self) -> &str {
        match self { Self::Type { name } | Self::Equal { name, .. } => name }
    }
    fn condition(&self, actual: Expr) -> Expr {
        match self {
            Self::Type { .. } => math::domains::source_type_guard::is_native_text(actual),
            // Narrow to native text, not String(actual) or loose equality.
            // A strict match to this literal is false for every non-text tag.
            Self::Equal { literal, .. } => b::eq(actual, literal.to_expression()),
        }
    }
    fn narrowed(&self, actual: Expr) -> Expr {
        match self {
            Self::Type { .. } => math::domains::source_type_guard::certified_text(actual),
            Self::Equal { literal, .. } => literal.to_expression(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum TextValue {
    Binding { name: String },
    Literal { value: WireText },
    Concat { left: Box<Self>, right: Box<Self> },
}

impl NativeTextFlow {
    pub(super) fn declared_throws(&self, declarations: &BTreeMap<String, String>) -> bool {
        match self {
            Self::If { then, otherwise, .. } =>
                then.declared_throws(declarations) && otherwise.declared_throws(declarations),
            Self::Return { .. } => true,
            Self::Throw { class, .. } => declarations.get(class.name()).is_some_and(|domain| domain == "text"),
        }
    }

    /// Guard for this exact source throw, including the tests preceding it.
    pub(super) fn throw_domain(&self, target: &str, payload: &Expr) -> Expr {
        match self {
            Self::If { condition, then, otherwise } => {
                let input = b::param(condition.name(), ScalarSymbolType::UntrustedSourceValue);
                let test = condition.condition(input);
                b::or(b::and(test.clone(), then.throw_domain(target, payload)),
                    b::and(b::not(test), otherwise.throw_domain(target, payload)))
            }
            Self::Return { .. } => b::lit_bool(false),
            Self::Throw { class, message } => b::lit_bool(class.name() == target && &message.to_expression() == payload),
        }
    }
}

impl Engine<'_> {
    fn flow_text_value(&mut self, value: &TextValue, scoped: &Env) -> Result<Expr> {
        self.budget()?;
        match value {
            TextValue::Binding { name } => scoped.get(name).filter(|value| value.ty() == Ty::Text)
                .ok_or_else(|| fail("text result requires the exact binding proved to be native text"))?.scalar(),
            TextValue::Literal { value } => Ok(value.to_expression()),
            TextValue::Concat { left, right } => {
                let left = self.flow_text_value(left, scoped)?;
                let right = self.flow_text_value(right, scoped)?;
                Ok(self.normalize_execution_value(b::add(left, right)))
            }
        }
    }

    pub(super) fn source_text_flow(
        &mut self, flow: &NativeTextFlow, inputs: &Env, scoped: &Env, path: Expr,
    ) -> Result<Completion> {
        self.budget()?;
        match flow {
            NativeTextFlow::Return { value } => Ok(Completion::Return(self.flow_text_value(value, scoped)?)),
            NativeTextFlow::Throw { class, message } =>
                Ok(Completion::Throw { class: class.name().into(), payload: Some(message.to_expression()) }),
            NativeTextFlow::If { condition, then, otherwise } => {
                let name = condition.name();
                let value = inputs.get(name).filter(|value| value.ty() == Ty::UnknownInput)
                    .ok_or_else(|| fail("native text test requires the exact unknown input"))?.scalar()?;
                let guard = self.condition_under_path(condition.condition(value.clone()), &path);
                let true_scope = |scope: &Env| {
                    let mut scope = scope.clone();
                    scope.insert(name.into(), Value::Term(Ty::Text, condition.narrowed(value)));
                    scope
                };
                match guard.as_bool() {
                    Some(true) => self.source_text_flow(then, inputs, &true_scope(scoped), path),
                    Some(false) => self.source_text_flow(otherwise, inputs, scoped, path),
                    None => {
                        let yes = self.source_text_flow(then, inputs, &true_scope(scoped), b::and(path.clone(), guard.clone()))?;
                        let no = self.source_text_flow(otherwise, inputs, scoped, b::and(path, b::not(guard.clone())))?;
                        realtime_state::join_completion(guard, yes, no)
                            .ok_or_else(|| fail("unsupported native text flow completion"))
                    }
                }
            }
        }
    }
}
