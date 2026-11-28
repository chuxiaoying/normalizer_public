//! A closed family of source-owned, immutable, generic products. The constructor
//! proof maps real source parameters to declared fields; actual type arguments
//! supply their types. Nothing is inferred from a class or field name.
use simulator_parser::{JavaClass, JavaExpr as E, JavaProgram, JavaStatement as S, JavaType};

#[derive(Debug)]
pub(crate) struct Product<'a> {
    class: &'a JavaClass,
    field_from_argument: Vec<(&'a str, usize)>,
    argument_type_parameter: Vec<usize>,
}

pub(crate) fn prove(class: &JavaClass) -> Option<Product<'_>> {
    let parameters = class.type_parameters();
    if parameters.is_empty()
        || class.is_interface()
        || class.supertype().is_some()
        || !class.interfaces().is_empty()
        || !class.static_initializers().is_empty()
        || !class.annotations().is_empty()
    {
        return None;
    }
    let [constructor] = class.methods() else {
        return None;
    };
    if !constructor.is_constructor()
        || !constructor.type_parameters().is_empty()
        || !constructor.throws().is_empty()
        || !constructor.annotations().is_empty()
        || constructor.arity() != class.fields().len()
        || class.fields().is_empty()
    {
        return None;
    }
    // The Java parser has already bound distinct, non-reserved parameters.
    if !constructor
        .params()
        .iter()
        .all(|(_, name)| constructor.param_annotations(name).is_empty())
    {
        return None;
    }
    let param_slot = |ty: &JavaType| {
        let JavaType::Class(parameter) = ty else {
            return None;
        };
        parameters.iter().position(|name| name == parameter)
    };
    let argument_type_parameter = constructor
        .params()
        .iter()
        .map(|(ty, _)| param_slot(ty))
        .collect::<Option<Vec<_>>>()?;
    let mut field_from_argument = Vec::new();
    for statement in constructor.body()? {
        let S::Assign {
            target: E::FieldAccess { target, field },
            value: E::Ident(argument),
            is_compound: false,
        } = statement
        else {
            return None;
        };
        if !matches!(target.as_ref(), E::Ident(receiver) if receiver == "this")
            || field_from_argument
                .iter()
                .any(|(prior, _)| *prior == field.as_str())
        {
            return None;
        }
        let (index, _) = constructor
            .params()
            .iter()
            .enumerate()
            .find(|(_, (_, name))| name == argument)?;
        let field = class
            .fields()
            .iter()
            .find(|declaration| declaration.name() == field)?;
        if field.is_static()
            || !field.is_final()
            || field.has_initializer()
            || !field.annotations().is_empty()
            || param_slot(field.java_type()) != Some(argument_type_parameter[index])
        {
            return None;
        }
        field_from_argument.push((field.name(), index));
    }
    if field_from_argument.len() != class.fields().len() {
        return None;
    }
    Some(Product {
        class,
        field_from_argument,
        argument_type_parameter,
    })
}

#[derive(Debug)]
pub(crate) struct InstantiatedProduct<'program, 'arguments> {
    proof: Product<'program>,
    arguments: &'arguments [JavaType],
}

pub(crate) fn instantiated<'program, 'arguments>(
    program: &'program JavaProgram,
    ty: &'arguments JavaType,
) -> Option<InstantiatedProduct<'program, 'arguments>> {
    let JavaType::Generic(name, args) = ty else {
        return None;
    };
    let class = program.find_class(name)?;
    if args.len() != class.type_parameters().len()
        || !args.iter().all(|arg| {
            matches!(arg, JavaType::Class(name)
            if name == "java.lang.String" || name == "String" && program.find_class(name).is_none())
        })
    {
        return None;
    }
    Some(InstantiatedProduct {
        proof: prove(class)?,
        arguments: args,
    })
}

pub(crate) fn same_instantiation(program: &JavaProgram, a: &JavaType, b: &JavaType) -> bool {
    let (Some(a), Some(b)) = (instantiated(program, a), instantiated(program, b)) else {
        return false;
    };
    // All presently admitted arguments resolve to the one java.lang.String
    // value type, including an unshadowed short spelling.
    a.name() == b.name()
}

impl<'program> InstantiatedProduct<'program, '_> {
    pub(crate) fn name(&self) -> String {
        self.proof.class.full_name()
    }

    pub(crate) fn bindings(&self) -> &[(&'program str, usize)] {
        &self.proof.field_from_argument
    }

    pub(crate) fn arity(&self) -> usize {
        self.proof.argument_type_parameter.len()
    }

    pub(crate) fn field_type(&self, field: &str) -> Option<JavaType> {
        let (_, index) = self
            .proof
            .field_from_argument
            .iter()
            .find(|(name, _)| *name == field)?;
        self.arguments
            .get(self.proof.argument_type_parameter[*index])
            .cloned()
    }
}
